use adventuresim_core::strategic_time::{
    MINUTES_PER_DAY, MINUTES_PER_YEAR, allocated_schedule_minutes,
    convalescence_minutes as calculate_convalescence_minutes,
    elapsed_official_minutes as calculate_elapsed_official_minutes, healed_health,
    training_hours_increment,
};
use spacetimedb::{ReducerContext, ScheduleAt, Table, reducer, table};

use crate::character::character;
use crate::{CharacterLimbs, CharacterSkills, character_limbs, character_skills};

pub const INN_GOLD_PER_DAY: u32 = 1;
/// The current authoritative strategic time. `official_minutes` is absolute;
/// calendar presentation wraps it into years without making comparisons wrap.
#[derive(Clone, Debug)]
#[table(name = world_clock, public)]
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
#[table(name = world_clock_schedule, scheduled(refresh_world_clock))]
pub struct WorldClockSchedule {
    #[primary_key]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
}

#[derive(Clone, Debug)]
#[table(name = character_time, public)]
pub struct CharacterTime {
    #[primary_key]
    pub character_id: u64,
    pub minutes: u64,
}

/// A 24-hour daily budget. Leisure is always the unallocated remainder.
#[derive(Clone, Debug)]
#[table(name = character_training_schedule, public)]
pub struct CharacterTrainingSchedule {
    #[primary_key]
    pub character_id: u64,
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
    pub labor_minutes: u16,
}

impl CharacterTrainingSchedule {
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
    if ctx.sender != ctx.identity() {
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
    character_time.minutes = character_time.minutes.saturating_add(minutes);
    ctx.db
        .character_time()
        .character_id()
        .update(character_time);
    Ok(())
}

fn default_schedule(character_id: u64) -> CharacterTrainingSchedule {
    CharacterTrainingSchedule {
        character_id,
        melee_minutes: 0,
        dodge_minutes: 0,
        block_minutes: 0,
        ranged_minutes: 0,
        will_minutes: 0,
        charisma_minutes: 0,
        medicine_minutes: 0,
        faith_minutes: 0,
        stealth_minutes: 0,
        balance_minutes: 0,
        surgeon_minutes: 0,
        labor_minutes: 0,
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

fn apply_training(
    skills: &mut CharacterSkills,
    schedule: &CharacterTrainingSchedule,
    elapsed: u64,
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
        apply_training(&mut skills, &schedule, training_elapsed);
        ctx.db.character_skills().character_id().update(skills);
    }

    character_time.minutes += elapsed;
    ctx.db
        .character_time()
        .character_id()
        .update(character_time);
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
    apply_training(&mut skills, &schedule, elapsed);
    ctx.db.character_skills().character_id().update(skills);
    character_time.minutes = target_minutes;
    ctx.db
        .character_time()
        .character_id()
        .update(character_time);
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
) -> Result<(), String> {
    if synchronize_character(ctx, character_id)? {
        return Ok(());
    }
    let schedule = CharacterTrainingSchedule {
        character_id,
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
    };
    if schedule.allocated_minutes() > MINUTES_PER_DAY {
        return Err("Training, labor, and leisure must fit within 24 hours".into());
    }
    ctx.db
        .character_training_schedule()
        .character_id()
        .update(schedule);
    Ok(())
}
