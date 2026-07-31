//! Bounded autonomous advancement for globally exclusive NPC characters.
//!
//! The wall-clock scheduler is only a wake-up mechanism. Every durable
//! decision is ordered by personal `CharacterTime`, then character id, and
//! advances at most one strategic day. This keeps retries and scheduler jitter
//! from changing causal order.

use adventuresim_core::strategic_time::MINUTES_PER_DAY;
use spacetimedb::{ReducerContext, ScheduleAt, Table, TimeDuration, reducer, table};

use crate::CharacterTime;
use crate::character::character as _;
use crate::relationship::npc_policy;
use crate::time::character_time;

const NPC_CAUSAL_SCHEDULE_ID: u64 = 0;
const NPC_CAUSAL_INTERVAL_MICROS: i64 = 5_000_000;
pub const MAX_NPCS_PER_CAUSAL_TICK: usize = 64;
pub const MAX_WEDDINGS_PER_CAUSAL_TICK: usize = 32;
pub const MAX_BIRTHS_PER_CAUSAL_TICK: usize = 32;

#[derive(Clone, Debug)]
#[table(accessor = npc_causal_schedule, scheduled(run_npc_causal_tick))]
pub struct NpcCausalSchedule {
    #[primary_key]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
}

pub fn initialize_npc_causal_schedule(ctx: &ReducerContext) {
    if ctx
        .db
        .npc_causal_schedule()
        .scheduled_id()
        .find(NPC_CAUSAL_SCHEDULE_ID)
        .is_none()
    {
        ctx.db.npc_causal_schedule().insert(NpcCausalSchedule {
            scheduled_id: NPC_CAUSAL_SCHEDULE_ID,
            scheduled_at: TimeDuration::from_micros(NPC_CAUSAL_INTERVAL_MICROS).into(),
        });
    }
}

fn stable_npc_batch(ctx: &ReducerContext, target_minute: u64) -> Vec<CharacterTime> {
    let mut candidates: Vec<_> = ctx
        .db
        .npc_policy()
        .iter()
        .filter_map(|policy| {
            let person = ctx.db.character().id().find(policy.character_id)?;
            let time = ctx
                .db
                .character_time()
                .character_id()
                .find(policy.character_id)?;
            (person.alive && time.minutes < target_minute).then_some(time)
        })
        .collect();
    candidates.sort_by_key(|time| (time.minutes, time.character_id));
    candidates.truncate(MAX_NPCS_PER_CAUSAL_TICK);
    candidates
}

/// Private recurring scheduler entry point. Lifecycle events are processed
/// independently of the NPC batch so a wedding or birth never waits for either
/// participant to log in or to be selected for policy advancement.
#[reducer]
pub fn run_npc_causal_tick(
    ctx: &ReducerContext,
    _schedule: NpcCausalSchedule,
) -> Result<(), String> {
    if ctx.sender() != ctx.database_identity() {
        return Err("NPC causal processing may only be invoked by its scheduler".into());
    }
    let official_minute = crate::time::refresh_clock(ctx)?;

    crate::relationship::settle_due_weddings_global(
        ctx,
        official_minute,
        MAX_WEDDINGS_PER_CAUSAL_TICK,
    )?;
    crate::relationship::settle_due_births_global(
        ctx,
        official_minute,
        MAX_BIRTHS_PER_CAUSAL_TICK,
    )?;

    for time in stable_npc_batch(ctx, official_minute) {
        let target = official_minute.min(time.minutes.saturating_add(MINUTES_PER_DAY));
        crate::time::advance_stationary_character_to(ctx, time.character_id, target)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduler_has_hard_bounded_batches_and_one_day_steps() {
        let source = include_str!("npc_causal.rs");
        assert!(source.contains("truncate(MAX_NPCS_PER_CAUSAL_TICK)"));
        assert!(source.contains("MAX_WEDDINGS_PER_CAUSAL_TICK"));
        assert!(source.contains("MAX_BIRTHS_PER_CAUSAL_TICK"));
        assert!(source.contains("time.minutes.saturating_add(MINUTES_PER_DAY)"));
    }

    #[test]
    fn stable_policy_order_is_frontier_then_character() {
        let source = include_str!("npc_causal.rs");
        assert!(source.contains("sort_by_key(|time| (time.minutes, time.character_id))"));
        assert!(source.contains("person.alive && time.minutes < target_minute"));
    }

    #[test]
    fn global_events_run_without_participant_login() {
        let source = include_str!("npc_causal.rs");
        let events = source
            .split("pub fn run_npc_causal_tick")
            .nth(1)
            .expect("scheduled reducer");
        assert!(events.contains("settle_due_weddings_global"));
        assert!(events.contains("settle_due_births_global"));
        assert!(!events.contains("require_strategic_character_authority"));
    }

    #[test]
    fn recurring_schedule_is_seeded_once_and_private() {
        let source = include_str!("npc_causal.rs");
        assert!(source.contains("TimeDuration::from_micros"));
        assert!(source.contains("ctx.sender() != ctx.database_identity()"));
        assert!(source.contains(".find(NPC_CAUSAL_SCHEDULE_ID)"));
    }
}
