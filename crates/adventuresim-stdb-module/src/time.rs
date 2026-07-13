use spacetimedb::{ReducerContext, ScheduleAt, Table, Timestamp, reducer, table};

use crate::character::character;
use crate::{CharacterLimbs, CharacterSkills, character_limbs, character_skills};

pub const MINUTES_PER_DAY: u64 = 24 * 60;
pub const MINUTES_PER_YEAR: u64 = 365 * MINUTES_PER_DAY;
/// Natural recovery while taking full settlement downtime.  This is an MVP
/// rate until the wound and treatment systems supply more detailed healing.
pub const HEALTH_RECOVERED_PER_DAY: f32 = 0.05;
pub const INN_GOLD_PER_DAY: u32 = 1;
const CLOCK_TICK_MICROS: u64 = 1_000_000;

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

/// Keeps the clock fresh for readers. The value itself is derived from the
/// timestamp, so delayed scheduled calls do not slow game time.
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
        [
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
        ]
        .into_iter()
        .map(u64::from)
        .sum()
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
    if ctx
        .db
        .world_clock_schedule()
        .scheduled_id()
        .find(0)
        .is_none()
    {
        ctx.db.world_clock_schedule().insert(WorldClockSchedule {
            scheduled_id: 0,
            scheduled_at: std::time::Duration::from_micros(CLOCK_TICK_MICROS).into(),
        });
    }
}

fn elapsed_official_minutes(clock: &WorldClock, now: Timestamp) -> u64 {
    let elapsed_micros = now
        .to_micros_since_unix_epoch()
        .saturating_sub(clock.epoch_micros) as u128;
    // One real week per 365-day game year: 84/73 seconds per game minute.
    (elapsed_micros.saturating_mul(73) / 84_000_000) as u64
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
    let official_minutes = elapsed_official_minutes(&clock, ctx.timestamp);
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
    let _ = schedule;
    refresh_clock(ctx).map(|_| ())
}

pub fn initialize_character_time(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    ensure_character_time(ctx, character_id)
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
    let hours_per_day = |minutes: u16| f32::from(minutes) / 60.0;
    let days = elapsed as f32 / MINUTES_PER_DAY as f32;
    skills.melee_hours += days * hours_per_day(schedule.melee_minutes);
    skills.dodge_hours += days * hours_per_day(schedule.dodge_minutes);
    skills.block_hours += days * hours_per_day(schedule.block_minutes);
    skills.ranged_hours += days * hours_per_day(schedule.ranged_minutes);
    skills.will_hours += days * hours_per_day(schedule.will_minutes);
    skills.charisma_hours += days * hours_per_day(schedule.charisma_minutes);
    skills.medicine_hours += days * hours_per_day(schedule.medicine_minutes);
    skills.faith_hours += days * hours_per_day(schedule.faith_minutes);
    skills.stealth_hours += days * hours_per_day(schedule.stealth_minutes);
    skills.balance_hours += days * hours_per_day(schedule.balance_minutes);
    skills.surgeon_hours += days * hours_per_day(schedule.surgeon_minutes);
}

fn heal_limbs(limbs: &mut CharacterLimbs, elapsed: u64) {
    let recovery = elapsed as f32 / MINUTES_PER_DAY as f32 * HEALTH_RECOVERED_PER_DAY;
    for health in [
        &mut limbs.left_arm_health,
        &mut limbs.right_arm_health,
        &mut limbs.left_leg_health,
        &mut limbs.right_leg_health,
        &mut limbs.head_health,
        &mut limbs.chest_health,
        &mut limbs.stomach_health,
    ] {
        *health = (*health + recovery).min(1.0);
    }
}

fn convalescence_minutes(limbs: &CharacterLimbs) -> u64 {
    let lowest_health = [
        limbs.left_arm_health,
        limbs.right_arm_health,
        limbs.left_leg_health,
        limbs.right_leg_health,
        limbs.head_health,
        limbs.chest_health,
        limbs.stomach_health,
    ]
    .into_iter()
    .fold(1.0_f32, f32::min);
    if lowest_health >= 1.0 {
        0
    } else {
        ((1.0 - lowest_health) / HEALTH_RECOVERED_PER_DAY * MINUTES_PER_DAY as f32).ceil() as u64
    }
}

/// Spend completed game days at a settlement. Injuries receive all available
/// rest first; only the remaining time is eligible for scheduled training.
#[reducer]
pub fn rest_at_settlement(
    ctx: &ReducerContext,
    character_id: u64,
    requested_days: u16,
    at_inn: bool,
) -> Result<(), String> {
    ensure_character_time(ctx, character_id)?;
    let official_minutes = refresh_clock(ctx)?;
    let mut character_time = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character time record not found".to_string())?;
    let available_days = official_minutes.saturating_sub(character_time.minutes) / MINUTES_PER_DAY;
    let days = u64::from(requested_days).min(available_days);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn clock(epoch_micros: i64) -> WorldClock {
        WorldClock {
            id: 0,
            official_minutes: 0,
            epoch_micros,
        }
    }

    #[test]
    fn one_real_week_is_one_game_year() {
        let clock = clock(0);
        let one_week_micros = 7 * 24 * 60 * 60 * 1_000_000i64;
        assert_eq!(
            elapsed_official_minutes(
                &clock,
                Timestamp::from_micros_since_unix_epoch(one_week_micros)
            ),
            MINUTES_PER_YEAR
        );
    }

    #[test]
    fn training_uses_the_daily_minute_allocation() {
        let mut skills = CharacterSkills {
            character_id: 1,
            melee_hours: 0.0,
            dodge_hours: 0.0,
            block_hours: 0.0,
            ranged_hours: 0.0,
            will_hours: 0.0,
            charisma_hours: 0.0,
            medicine_hours: 0.0,
            faith_hours: 0.0,
            stealth_hours: 0.0,
            balance_hours: 0.0,
            surgeon_hours: 0.0,
        };
        let schedule = CharacterTrainingSchedule {
            character_id: 1,
            melee_minutes: 90,
            dodge_minutes: 30,
            block_minutes: 0,
            ranged_minutes: 0,
            will_minutes: 0,
            charisma_minutes: 0,
            medicine_minutes: 0,
            faith_minutes: 0,
            stealth_minutes: 0,
            balance_minutes: 0,
            surgeon_minutes: 0,
            labor_minutes: 480,
        };
        apply_training(&mut skills, &schedule, MINUTES_PER_DAY * 2);
        assert_eq!(skills.melee_hours, 3.0);
        assert_eq!(skills.dodge_hours, 1.0);
        assert_eq!(schedule.allocated_minutes(), 600);
    }

    #[test]
    fn convalescence_blocks_training_until_the_slowest_limb_recovers() {
        let limbs = CharacterLimbs {
            character_id: 1,
            left_arm_health: 0.9,
            right_arm_health: 1.0,
            left_leg_health: 1.0,
            right_leg_health: 1.0,
            head_health: 1.0,
            chest_health: 1.0,
            stomach_health: 1.0,
        };
        assert_eq!(convalescence_minutes(&limbs), MINUTES_PER_DAY * 2);
    }

    #[test]
    fn healing_is_capped_at_full_health() {
        let mut limbs = CharacterLimbs {
            character_id: 1,
            left_arm_health: 0.98,
            right_arm_health: 1.0,
            left_leg_health: 1.0,
            right_leg_health: 1.0,
            head_health: 1.0,
            chest_health: 1.0,
            stomach_health: 1.0,
        };
        heal_limbs(&mut limbs, MINUTES_PER_DAY);
        assert_eq!(limbs.left_arm_health, 1.0);
    }
}
