use adventuresim_core::prelude::*;
use adventuresim_core::strategic_time::{
    MINUTES_PER_DAY, MINUTES_PER_YEAR, allocated_schedule_minutes,
    convalescence_minutes as calculate_convalescence_minutes,
    elapsed_official_minutes as calculate_elapsed_official_minutes, healed_health,
    training_hours_increment,
};
use spacetimedb::{ReducerContext, ScheduleAt, SpacetimeType, Table, reducer, table};

use crate::capability::StrategicEquipment;
use crate::character::character;
use crate::{
    CharacterAttributes, CharacterLimbs, CharacterSkills, CharacterStats, character_attributes,
    character_equip, character_limbs, character_skills, character_stats, settlement,
};

pub const INN_GOLD_PER_DAY: u32 = 1;
/// The current authoritative strategic time. `official_minutes` is absolute;
/// calendar presentation wraps it into years without making comparisons wrap.
#[derive(Clone, Debug)]
#[table(accessor = world_clock, public)]
pub struct WorldClock {
    #[primary_key]
    pub id: u64,
    pub official_minutes: u64,
    pub epoch_micros: i64,
}

/// Legacy scheduler row retained so existing databases can migrate without
/// dropping a table. New clocks are derived on demand and no longer schedule
/// a write every second.
#[derive(Clone, Debug)]
#[table(accessor = world_clock_schedule, scheduled(refresh_world_clock))]
pub struct WorldClockSchedule {
    #[primary_key]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
}

#[derive(Clone, Debug)]
#[table(accessor = character_time, public)]
pub struct CharacterTime {
    #[primary_key]
    pub character_id: u64,
    pub minutes: u64,
}

/// One 24-hour daily budget. Leisure is always the unallocated remainder.
#[derive(Clone, Debug, Default, SpacetimeType)]
pub struct ScheduleAllocation {
    pub melee_minutes: u16,
    pub dodge_minutes: u16,
    pub block_minutes: u16,
    pub ranged_minutes: u16,
    pub will_minutes: u16,
    pub charisma_minutes: u16,
    pub medicine_minutes: u16,
    pub faith_minutes: u16,
    pub stealth_minutes: u16,
    pub balance_minutes: u16,
    pub surgeon_minutes: u16,
    /// Paid physical work; also trains Will at reduced speed.
    pub labor_minutes: u16,
    pub prayer_minutes: u16,
    pub thievery_minutes: u16,
    pub raiding_minutes: u16,
}

/// Separate daily plans for settlement downtime and strategic travel.
#[derive(Clone, Debug)]
#[table(accessor = character_training_schedule, public)]
pub struct CharacterTrainingSchedule {
    #[primary_key]
    pub character_id: u64,
    pub downtime: ScheduleAllocation,
    pub travel: ScheduleAllocation,
}

#[derive(Clone, Debug)]
#[table(accessor = character_notoriety, public)]
pub struct CharacterNotoriety {
    #[primary_key]
    pub character_id: u64,
    pub value: f32,
}

impl ScheduleAllocation {
    pub fn allocated_minutes(&self) -> u64 {
        allocated_schedule_minutes([
            self.melee_minutes,
            self.dodge_minutes,
            self.block_minutes,
            self.ranged_minutes,
            self.will_minutes,
            self.charisma_minutes,
            self.medicine_minutes,
            self.faith_minutes,
            self.stealth_minutes,
            self.balance_minutes,
            self.surgeon_minutes,
            self.labor_minutes,
            self.prayer_minutes,
            self.thievery_minutes,
            self.raiding_minutes,
        ])
    }
}

pub fn initialize_time(ctx: &ReducerContext) {
    if ctx.db.world_clock().id().find(0).is_none() {
        ctx.db.world_clock().insert(WorldClock {
            id: 0,
            official_minutes: 0,
            epoch_micros: ctx.timestamp.to_micros_since_unix_epoch(),
        });
    }
}

pub fn refresh_clock(ctx: &ReducerContext) -> Result<u64, String> {
    if ctx.db.world_clock().id().find(0).is_none() {
        initialize_time(ctx);
    }
    let mut clock = ctx
        .db
        .world_clock()
        .id()
        .find(0)
        .ok_or_else(|| "World clock is not initialized".to_string())?;
    let official_minutes = calculate_elapsed_official_minutes(
        clock.epoch_micros,
        ctx.timestamp.to_micros_since_unix_epoch(),
    );
    if official_minutes != clock.official_minutes {
        clock.official_minutes = official_minutes;
        ctx.db.world_clock().id().update(clock);
    }
    Ok(official_minutes)
}

#[reducer]
fn refresh_world_clock(ctx: &ReducerContext, schedule: WorldClockSchedule) -> Result<(), String> {
    if ctx.sender() != ctx.database_identity() {
        return Err("World clock may only be refreshed by its scheduler".into());
    }
    // Remove scheduler rows created by older module versions. The authoritative
    // value is calculated from `epoch_micros` whenever an action needs it, and
    // browsers advance their initial snapshot locally.
    ctx.db
        .world_clock_schedule()
        .scheduled_id()
        .delete(schedule.scheduled_id);
    Ok(())
}

pub fn initialize_character_time(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    ensure_character_time(ctx, character_id)
}

/// Record time spent travelling without applying settlement-only recovery or
/// training. Travel time belongs to the character's personal strategic clock.
pub fn advance_character_time(
    ctx: &ReducerContext,
    character_id: u64,
    minutes: u64,
) -> Result<(), String> {
    ensure_character_time(ctx, character_id)?;
    let mut character_time = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character time record not found".to_string())?;
    let starting_minute = character_time.minutes;
    character_time.minutes = character_time.minutes.saturating_add(minutes);
    ctx.db
        .character_time()
        .character_id()
        .update(character_time);
    let schedule = ctx
        .db
        .character_training_schedule()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character training schedule not found".to_string())?;
    let mut skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character skill record not found".to_string())?;
    let activities = activity_training_profile(ctx, character_id)?;
    apply_training(&mut skills, &schedule.travel, minutes, activities);
    ctx.db.character_skills().character_id().update(skills);
    crate::condition::apply_travel_condition(
        ctx,
        character_id,
        starting_minute,
        minutes,
        schedule.travel.prayer_minutes,
    )?;
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(())
}

fn default_schedule(character_id: u64) -> CharacterTrainingSchedule {
    CharacterTrainingSchedule {
        character_id,
        downtime: ScheduleAllocation::default(),
        travel: ScheduleAllocation::default(),
    }
}

fn ensure_character_time(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    let official_minutes = refresh_clock(ctx)?;
    if ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .is_none()
    {
        ctx.db.character_time().insert(CharacterTime {
            character_id,
            minutes: official_minutes,
        });
    }
    if ctx
        .db
        .character_training_schedule()
        .character_id()
        .find(character_id)
        .is_none()
    {
        ctx.db
            .character_training_schedule()
            .insert(default_schedule(character_id));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default)]
struct ActivityTrainingProfile {
    raiding_melee: bool,
    raiding_ranged: bool,
    raiding_block: bool,
    raiding_dodge: bool,
}

fn activity_training_profile(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<ActivityTrainingProfile, String> {
    let capability = crate::capability::evaluate_character(ctx, character_id)?;
    Ok(ActivityTrainingProfile {
        raiding_melee: capability.melee && !capability.ranged,
        raiding_ranged: capability.ranged,
        raiding_block: capability.half_armor
            || capability.three_quarter_armor
            || capability.full_armor,
        raiding_dodge: !capability.full_armor && !capability.three_quarter_armor,
    })
}

fn apply_training(
    skills: &mut CharacterSkills,
    schedule: &ScheduleAllocation,
    elapsed: u64,
    activities: ActivityTrainingProfile,
) {
    skills.melee_hours += training_hours_increment(elapsed, schedule.melee_minutes);
    skills.dodge_hours += training_hours_increment(elapsed, schedule.dodge_minutes);
    skills.block_hours += training_hours_increment(elapsed, schedule.block_minutes);
    skills.ranged_hours += training_hours_increment(elapsed, schedule.ranged_minutes);
    skills.will_hours += training_hours_increment(elapsed, schedule.will_minutes);
    skills.charisma_hours += training_hours_increment(elapsed, schedule.charisma_minutes);
    skills.medicine_hours += training_hours_increment(elapsed, schedule.medicine_minutes);
    skills.faith_hours += training_hours_increment(elapsed, schedule.faith_minutes);
    skills.stealth_hours += training_hours_increment(elapsed, schedule.stealth_minutes);
    skills.balance_hours += training_hours_increment(elapsed, schedule.balance_minutes);
    skills.surgeon_hours += training_hours_increment(elapsed, schedule.surgeon_minutes);
    skills.faith_hours += training_hours_increment(elapsed, schedule.prayer_minutes)
        * adventuresim_core::activity::ACTIVITY_TRAINING_RATE;
    skills.will_hours += training_hours_increment(elapsed, schedule.labor_minutes)
        * adventuresim_core::activity::ACTIVITY_TRAINING_RATE;
    skills.stealth_hours += training_hours_increment(elapsed, schedule.thievery_minutes)
        * adventuresim_core::activity::ACTIVITY_TRAINING_RATE;
    let raiding_training = training_hours_increment(elapsed, schedule.raiding_minutes)
        * adventuresim_core::activity::ACTIVITY_TRAINING_RATE;
    if activities.raiding_ranged {
        skills.ranged_hours += raiding_training;
    } else if activities.raiding_melee {
        skills.melee_hours += raiding_training;
    }
    if activities.raiding_block {
        skills.block_hours += raiding_training * 0.5;
    }
    if activities.raiding_dodge {
        skills.dodge_hours += raiding_training * 0.5;
    }
}

fn settlement_population_scale(population_level: i32, population_estimate: u32) -> f32 {
    if population_estimate > 0 {
        ((population_estimate as f32 + 1.0).ln() / 4.0).clamp(1.0, 4.0)
    } else {
        (population_level.max(1) as f32 / 2.0).clamp(0.5, 3.0)
    }
}

fn initialize_notoriety(ctx: &ReducerContext, character_id: u64) {
    if ctx
        .db
        .character_notoriety()
        .character_id()
        .find(character_id)
        .is_none()
    {
        ctx.db.character_notoriety().insert(CharacterNotoriety {
            character_id,
            value: 0.0,
        });
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ActivityRisks {
    pub thievery_discovery: f32,
    pub raiding_retaliation: f32,
}

fn apply_activity_outcomes(
    ctx: &ReducerContext,
    character_id: u64,
    schedule: &ScheduleAllocation,
    elapsed: u64,
) -> Result<ActivityRisks, String> {
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let Some(settlement_id) = character.current_settlement_id.as_ref() else {
        return Ok(ActivityRisks::default());
    };
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(settlement_id)
        .ok_or("Character's settlement not found")?;
    let attributes: CharacterAttributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .ok_or("Character attributes not found")?;
    let limbs = ctx
        .db
        .character_limbs()
        .character_id()
        .find(character_id)
        .ok_or("Character limbs not found")?;
    let skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Character skills not found")?;
    let stats: CharacterStats = ctx
        .db
        .character_stats()
        .character_id()
        .find(character_id)
        .ok_or("Character stats not found")?;
    let equip = ctx
        .db
        .character_equip()
        .character_id()
        .find(character_id)
        .ok_or("Character equipment not found")?;
    let equipment = StrategicEquipment::load(ctx, character_id, &equip);
    let strength = attributes.limb_attr_by_weight_by_parts(
        LimbAttribute::Strength,
        &limbs,
        LimbWeights::all_equal(),
    );
    let endurance = attributes.attr_by_parts(SimpleAttribute::Endurance, &limbs);
    let stealth = skills.skill_check_by_parts(
        Skill::Stealth,
        &attributes,
        &limbs,
        &stats,
        &equipment,
        LimbWeights::all_equal(),
    );
    let capability = crate::capability::evaluate_character(ctx, character_id)?;
    let days = elapsed as f32 / MINUTES_PER_DAY as f32;
    let hours = |minutes: u16| days * f32::from(minutes) / 60.0;
    let population =
        settlement_population_scale(settlement.population_level, settlement.population_estimate);
    let labor_hours = hours(schedule.labor_minutes);
    let thievery_hours = hours(schedule.thievery_minutes);
    let raiding_hours = hours(schedule.raiding_minutes);
    let combat = capability
        .weapon_precision
        .max(capability.athletics)
        .max(capability.endurance);
    let gold = labor_gold(labor_hours, strength, endurance)
        .saturating_add(thievery_gold(thievery_hours, population, stealth))
        .saturating_add(raiding_gold(raiding_hours, combat));
    if gold > 0 {
        let mut character = character;
        character.gold = character.gold.saturating_add(gold);
        ctx.db.character().id().update(character);
    }
    initialize_notoriety(ctx, character_id);
    let notoriety_gain =
        thievery_notoriety(thievery_hours, population, stealth) + raiding_notoriety(raiding_hours);
    if notoriety_gain > 0.0 {
        let mut notoriety = ctx
            .db
            .character_notoriety()
            .character_id()
            .find(character_id)
            .ok_or("Character notoriety not found")?;
        notoriety.value += notoriety_gain;
        ctx.db
            .character_notoriety()
            .character_id()
            .update(notoriety);
    }
    Ok(ActivityRisks {
        thievery_discovery: thievery_discovery_chance(thievery_hours, population, stealth),
        raiding_retaliation: raiding_retaliation_chance(raiding_hours),
    })
}

fn heal_limbs(limbs: &mut CharacterLimbs, elapsed: u64) {
    for health in [
        &mut limbs.left_arm_health,
        &mut limbs.right_arm_health,
        &mut limbs.left_leg_health,
        &mut limbs.right_leg_health,
        &mut limbs.head_health,
        &mut limbs.chest_health,
        &mut limbs.stomach_health,
    ] {
        *health = healed_health(*health, elapsed);
    }
}

fn convalescence_minutes(limbs: &CharacterLimbs) -> u64 {
    calculate_convalescence_minutes([
        limbs.left_arm_health,
        limbs.right_arm_health,
        limbs.left_leg_health,
        limbs.right_leg_health,
        limbs.head_health,
        limbs.chest_health,
        limbs.stomach_health,
    ])
}

/// Spend completed game days at a settlement. Injuries receive all selected
/// rest first; only the remaining time is eligible for scheduled training.
#[reducer]
pub fn rest_at_settlement(
    ctx: &ReducerContext,
    character_id: u64,
    requested_days: u16,
    at_inn: bool,
) -> Result<(), String> {
    ensure_character_time(ctx, character_id)?;
    let _ = refresh_clock(ctx)?;
    let mut character_time = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character time record not found".to_string())?;
    let days = u64::from(requested_days);
    if days == 0 {
        return Ok(());
    }

    let cost = (days as u32).saturating_mul(INN_GOLD_PER_DAY);
    if at_inn {
        let mut character = ctx
            .db
            .character()
            .id()
            .find(character_id)
            .ok_or_else(|| "Character not found".to_string())?;
        if character.gold < cost {
            return Err("Not enough gold to pay for the inn stay".into());
        }
        character.gold -= cost;
        ctx.db.character().id().update(character);
    }

    let elapsed = days * MINUTES_PER_DAY;
    let mut limbs = ctx
        .db
        .character_limbs()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character limb record not found".to_string())?;
    let convalescing = convalescence_minutes(&limbs).min(elapsed);
    heal_limbs(&mut limbs, elapsed);
    ctx.db.character_limbs().character_id().update(limbs);

    let training_elapsed = elapsed.saturating_sub(convalescing);
    if training_elapsed > 0 {
        let schedule = ctx
            .db
            .character_training_schedule()
            .character_id()
            .find(character_id)
            .ok_or_else(|| "Character training schedule not found".to_string())?;
        let mut skills = ctx
            .db
            .character_skills()
            .character_id()
            .find(character_id)
            .ok_or_else(|| "Character skill record not found".to_string())?;
        let activities = activity_training_profile(ctx, character_id)?;
        apply_training(
            &mut skills,
            &schedule.downtime,
            training_elapsed,
            activities,
        );
        ctx.db.character_skills().character_id().update(skills);
        let risks =
            apply_activity_outcomes(ctx, character_id, &schedule.downtime, training_elapsed)?;
        crate::strategic::maybe_trigger_activity_incident(ctx, character_id, risks)?;
    }

    character_time.minutes += elapsed;
    ctx.db
        .character_time()
        .character_id()
        .update(character_time);
    crate::condition::apply_rest_condition(ctx, character_id, elapsed)?;
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(())
}

/// Advance through elapsed time. Returns true when a character was forced to
/// catch up from more than a year behind; callers should skip their action.
pub fn synchronize_character(ctx: &ReducerContext, character_id: u64) -> Result<bool, String> {
    ensure_character_time(ctx, character_id)?;
    let official_minutes = refresh_clock(ctx)?;
    let mut character_time = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character time record not found".to_string())?;
    let forced_catch_up =
        official_minutes.saturating_sub(character_time.minutes) > MINUTES_PER_YEAR;
    let target_minutes = if forced_catch_up {
        official_minutes.saturating_sub(MINUTES_PER_YEAR)
    } else {
        official_minutes
    };
    let elapsed = target_minutes.saturating_sub(character_time.minutes);
    if elapsed == 0 {
        return Ok(forced_catch_up);
    }
    let schedule = ctx
        .db
        .character_training_schedule()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character training schedule not found".to_string())?;
    let mut skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character skill record not found".to_string())?;
    let activities = activity_training_profile(ctx, character_id)?;
    apply_training(&mut skills, &schedule.downtime, elapsed, activities);
    ctx.db.character_skills().character_id().update(skills);
    let risks = apply_activity_outcomes(ctx, character_id, &schedule.downtime, elapsed)?;
    crate::strategic::maybe_trigger_activity_incident(ctx, character_id, risks)?;
    character_time.minutes = target_minutes;
    ctx.db
        .character_time()
        .character_id()
        .update(character_time);
    crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    Ok(forced_catch_up)
}

/// Explicitly synchronize an accessed character before strategic UI reads.
#[reducer]
pub fn synchronize_character_time(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    synchronize_character(ctx, character_id).map(|_| ())
}

#[allow(clippy::too_many_arguments)]
#[reducer]
pub fn update_training_schedule(
    ctx: &ReducerContext,
    character_id: u64,
    melee_minutes: u16,
    dodge_minutes: u16,
    block_minutes: u16,
    ranged_minutes: u16,
    will_minutes: u16,
    charisma_minutes: u16,
    medicine_minutes: u16,
    faith_minutes: u16,
    stealth_minutes: u16,
    balance_minutes: u16,
    surgeon_minutes: u16,
    labor_minutes: u16,
    prayer_minutes: u16,
    thievery_minutes: u16,
    raiding_minutes: u16,
    travel_melee_minutes: u16,
    travel_dodge_minutes: u16,
    travel_block_minutes: u16,
    travel_ranged_minutes: u16,
    travel_will_minutes: u16,
    travel_charisma_minutes: u16,
    travel_medicine_minutes: u16,
    travel_faith_minutes: u16,
    travel_stealth_minutes: u16,
    travel_balance_minutes: u16,
    travel_surgeon_minutes: u16,
    travel_labor_minutes: u16,
    travel_prayer_minutes: u16,
    travel_thievery_minutes: u16,
    travel_raiding_minutes: u16,
) -> Result<(), String> {
    if synchronize_character(ctx, character_id)? {
        return Ok(());
    }
    let schedule = CharacterTrainingSchedule {
        character_id,
        downtime: ScheduleAllocation {
            melee_minutes,
            dodge_minutes,
            block_minutes,
            ranged_minutes,
            will_minutes,
            charisma_minutes,
            medicine_minutes,
            faith_minutes,
            stealth_minutes,
            balance_minutes,
            surgeon_minutes,
            labor_minutes,
            prayer_minutes,
            thievery_minutes,
            raiding_minutes,
        },
        travel: ScheduleAllocation {
            melee_minutes: travel_melee_minutes,
            dodge_minutes: travel_dodge_minutes,
            block_minutes: travel_block_minutes,
            ranged_minutes: travel_ranged_minutes,
            will_minutes: travel_will_minutes,
            charisma_minutes: travel_charisma_minutes,
            medicine_minutes: travel_medicine_minutes,
            faith_minutes: travel_faith_minutes,
            stealth_minutes: travel_stealth_minutes,
            balance_minutes: travel_balance_minutes,
            surgeon_minutes: travel_surgeon_minutes,
            labor_minutes: travel_labor_minutes,
            prayer_minutes: travel_prayer_minutes,
            thievery_minutes: travel_thievery_minutes,
            raiding_minutes: travel_raiding_minutes,
        },
    };
    if schedule.downtime.allocated_minutes() > MINUTES_PER_DAY
        || schedule.travel.allocated_minutes() > MINUTES_PER_DAY
    {
        return Err("Each downtime and travel plan must fit within 24 hours".into());
    }
    ctx.db
        .character_training_schedule()
        .character_id()
        .update(schedule);
    crate::condition::refresh_character_strategic_condition(ctx, character_id).map(|_| ())
}
