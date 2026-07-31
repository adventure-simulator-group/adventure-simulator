//! Bounded autonomous advancement for globally exclusive NPC characters.
//!
//! The wall-clock scheduler is only a wake-up mechanism. Every durable
//! decision is ordered by personal `CharacterTime`, then character id, and
//! advances at most one strategic day. This keeps retries and scheduler jitter
//! from changing causal order.

use adventuresim_core::courtship::ADULT_AGE_YEARS;
use adventuresim_core::strategic_time::MINUTES_PER_DAY;
use spacetimedb::{ReducerContext, ScheduleAt, SpacetimeType, Table, TimeDuration, reducer, table};

use crate::CharacterTime;
use crate::character::character as _;
use crate::personality::{Sex, character_personality as _};
use crate::relationship::npc_policy;
use crate::settlement_population::settlement_resident_presence;
use crate::time::{ScheduleAllocation, character_time, character_training_schedule};

const NPC_CAUSAL_SCHEDULE_ID: u64 = 0;
const NPC_CAUSAL_INTERVAL_MICROS: i64 = 5_000_000;
pub const MAX_NPCS_PER_CAUSAL_TICK: usize = 64;
pub const MAX_LIFECYCLE_EVENTS_PER_CAUSAL_TICK: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum NpcPolicyDecisionPhase {
    Schedule,
    Housing,
    Romance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum NpcPolicyDecisionOutcome {
    ScheduleInitialized,
    SchedulePreserved,
    HousingAlreadyOccupied,
    HousingRecovered,
    HousingRentedCheap,
    HousingRentedModerate,
    HousingRentedFancy,
    HousingUnaffordable,
    HousingNotAtHome,
    Ineligible,
    RomanceEstablishedFormal,
    RomanceEstablishedInformal,
    RomanceNoCandidate,
}

/// Private durable evidence for one autonomous decision phase. The compound
/// primary key makes scheduler retries and wall-clock jitter idempotent.
#[derive(Clone, Debug)]
#[table(accessor = npc_policy_decision_receipt)]
pub struct NpcPolicyDecisionReceipt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    pub day: u64,
    pub phase: NpcPolicyDecisionPhase,
    pub outcome: NpcPolicyDecisionOutcome,
    pub target_character_id: Option<u64>,
    pub decided_minute: u64,
}

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

fn stable_npc_cohort(ctx: &ReducerContext, target_minute: u64) -> Vec<CharacterTime> {
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
    let Some(frontier) = candidates.first().map(|time| time.minutes) else {
        return candidates;
    };
    candidates.retain(|time| time.minutes == frontier);
    candidates.truncate(MAX_NPCS_PER_CAUSAL_TICK);
    candidates
}

fn receipt_id(character_id: u64, day: u64, phase: NpcPolicyDecisionPhase) -> String {
    format!("npc-policy:{character_id}:{day}:{phase:?}")
}

fn phase_already_decided(
    ctx: &ReducerContext,
    character_id: u64,
    day: u64,
    phase: NpcPolicyDecisionPhase,
) -> bool {
    ctx.db
        .npc_policy_decision_receipt()
        .id()
        .find(&receipt_id(character_id, day, phase))
        .is_some()
}

fn record_decision(
    ctx: &ReducerContext,
    character_id: u64,
    day: u64,
    phase: NpcPolicyDecisionPhase,
    outcome: NpcPolicyDecisionOutcome,
    target_character_id: Option<u64>,
    decided_minute: u64,
) {
    let id = receipt_id(character_id, day, phase);
    if ctx
        .db
        .npc_policy_decision_receipt()
        .id()
        .find(&id)
        .is_none()
    {
        ctx.db
            .npc_policy_decision_receipt()
            .insert(NpcPolicyDecisionReceipt {
                id,
                character_id,
                day,
                phase,
                outcome,
                target_character_id,
                decided_minute,
            });
    }
}

fn schedule_is_unallocated(schedule: &ScheduleAllocation) -> bool {
    schedule.allocated_minutes() == 0
        && schedule.apprenticeship_organization_id.is_none()
        && schedule.practice_organization_id.is_none()
}

fn has_initialized_schedule_decision(ctx: &ReducerContext, character_id: u64) -> bool {
    ctx.db
        .npc_policy_decision_receipt()
        .character_id()
        .filter(character_id)
        .any(|receipt| {
            receipt.phase == NpcPolicyDecisionPhase::Schedule
                && matches!(
                    receipt.outcome,
                    NpcPolicyDecisionOutcome::ScheduleInitialized
                        | NpcPolicyDecisionOutcome::SchedulePreserved
                )
        })
}

fn initialize_saved_schedule_once(
    ctx: &ReducerContext,
    character_id: u64,
    policy_seed: u64,
    minute: u64,
) -> Result<(), String> {
    let day = minute / MINUTES_PER_DAY;
    if phase_already_decided(ctx, character_id, day, NpcPolicyDecisionPhase::Schedule) {
        return Ok(());
    }
    let age_years = crate::relationship::effective_age_years(ctx, character_id, minute)
        .ok_or("NPC policy character is missing age chronology")?;
    if age_years < ADULT_AGE_YEARS {
        // Dependents retain the empty schedule created with their full
        // Character. Childhood work and education are separate follow-ups;
        // never install the adult labor/socializing policy before adulthood.
        record_decision(
            ctx,
            character_id,
            day,
            NpcPolicyDecisionPhase::Schedule,
            NpcPolicyDecisionOutcome::Ineligible,
            None,
            minute,
        );
        return Ok(());
    }
    let mut schedule = ctx
        .db
        .character_training_schedule()
        .character_id()
        .find(character_id)
        .ok_or("NPC policy character is missing a saved schedule")?;
    let previously_initialized = has_initialized_schedule_decision(ctx, character_id);
    let outcome = if !previously_initialized && schedule_is_unallocated(&schedule.downtime) {
        let initial =
            adventuresim_core::npc_policy::initial_npc_schedule(character_id, policy_seed);
        schedule.downtime = ScheduleAllocation {
            reading_minutes: initial.reading_minutes,
            combat_training_minutes: initial.combat_training_minutes,
            carousing_minutes: initial.carousing_minutes,
            socializing_minutes: initial.socializing_minutes,
            apprenticeship_minutes: initial.apprenticeship_minutes,
            apprenticeship_organization_id: None,
            profession_practice_minutes: initial.profession_practice_minutes,
            practice_organization_id: None,
            labor_minutes: initial.labor,
            prayer_minutes: initial.prayer,
            thievery_minutes: initial.thievery,
            raiding_minutes: initial.raiding,
        };
        ctx.db
            .character_training_schedule()
            .character_id()
            .update(schedule);
        NpcPolicyDecisionOutcome::ScheduleInitialized
    } else {
        NpcPolicyDecisionOutcome::SchedulePreserved
    };
    record_decision(
        ctx,
        character_id,
        day,
        NpcPolicyDecisionPhase::Schedule,
        outcome,
        None,
        minute,
    );
    Ok(())
}

fn settle_housing_decision(
    ctx: &ReducerContext,
    character_id: u64,
    home_settlement_id: &str,
    minute: u64,
) -> Result<(), String> {
    let day = minute / MINUTES_PER_DAY;
    if phase_already_decided(ctx, character_id, day, NpcPolicyDecisionPhase::Housing) {
        return Ok(());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("NPC policy character not found")?;
    let outcome = if !character.alive || character.age_years < ADULT_AGE_YEARS {
        NpcPolicyDecisionOutcome::Ineligible
    } else {
        match crate::residence::settle_npc_residence(ctx, character_id, home_settlement_id)? {
            crate::residence::NpcResidenceOutcome::AlreadyOccupant => {
                NpcPolicyDecisionOutcome::HousingAlreadyOccupied
            }
            crate::residence::NpcResidenceOutcome::RecoveredOwner => {
                NpcPolicyDecisionOutcome::HousingRecovered
            }
            crate::residence::NpcResidenceOutcome::Rented(tier) => match tier {
                crate::residence::ResidenceTier::Cheap => {
                    NpcPolicyDecisionOutcome::HousingRentedCheap
                }
                crate::residence::ResidenceTier::Moderate => {
                    NpcPolicyDecisionOutcome::HousingRentedModerate
                }
                crate::residence::ResidenceTier::Fancy => {
                    NpcPolicyDecisionOutcome::HousingRentedFancy
                }
            },
            crate::residence::NpcResidenceOutcome::NoAffordableOffer => {
                NpcPolicyDecisionOutcome::HousingUnaffordable
            }
            crate::residence::NpcResidenceOutcome::NotAtHome => {
                NpcPolicyDecisionOutcome::HousingNotAtHome
            }
        }
    };
    record_decision(
        ctx,
        character_id,
        day,
        NpcPolicyDecisionPhase::Housing,
        outcome,
        None,
        minute,
    );
    Ok(())
}

fn settle_romance_decision(
    ctx: &ReducerContext,
    character_id: u64,
    policy_seed: u64,
    minute: u64,
) -> Result<(), String> {
    let day = minute / MINUTES_PER_DAY;
    if phase_already_decided(ctx, character_id, day, NpcPolicyDecisionPhase::Romance) {
        return Ok(());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("NPC policy character not found")?;
    let Some(actor_presence) = ctx
        .db
        .settlement_resident_presence()
        .character_id()
        .find(character_id)
        .filter(|presence| crate::settlement_population::npc_is_present(presence, minute))
    else {
        record_decision(
            ctx,
            character_id,
            day,
            NpcPolicyDecisionPhase::Romance,
            NpcPolicyDecisionOutcome::RomanceNoCandidate,
            None,
            minute,
        );
        return Ok(());
    };
    if !character.alive
        || character.age_years < ADULT_AGE_YEARS
        || character.current_settlement_id.as_deref() != Some(&actor_presence.settlement_id)
    {
        record_decision(
            ctx,
            character_id,
            day,
            NpcPolicyDecisionPhase::Romance,
            NpcPolicyDecisionOutcome::Ineligible,
            None,
            minute,
        );
        return Ok(());
    }
    let candidates = ctx
        .db
        .settlement_resident_presence()
        .settlement_id()
        .filter(&actor_presence.settlement_id)
        .filter(|presence| {
            presence.character_id != character_id
                && crate::settlement_population::npc_is_present(presence, minute)
        })
        .filter_map(|presence| {
            ctx.db
                .npc_policy()
                .character_id()
                .find(presence.character_id)
                .map(|policy| adventuresim_core::npc_policy::NpcCandidate {
                    character_id: presence.character_id,
                    policy_seed: policy.policy_seed,
                })
        });
    let candidates = adventuresim_core::npc_policy::stable_candidate_order(
        character_id,
        policy_seed,
        day,
        candidates,
    );
    for candidate in candidates {
        let actor_sex = ctx
            .db
            .character_personality()
            .character_id()
            .find(character_id)
            .ok_or("NPC romance actor is missing personality")?
            .sex;
        let candidate_sex = ctx
            .db
            .character_personality()
            .character_id()
            .find(candidate.character_id)
            .ok_or("NPC romance candidate is missing personality")?
            .sex;
        let (suitor_id, partner_id) = npc_courtship_roles(
            character_id,
            actor_sex,
            candidate.character_id,
            candidate_sex,
        );
        let outcome =
            crate::relationship::establish_npc_courtship_and_wedding(ctx, suitor_id, partner_id)?;
        let receipt_outcome = match outcome {
            crate::relationship::NpcCourtshipOutcome::Formal => {
                NpcPolicyDecisionOutcome::RomanceEstablishedFormal
            }
            crate::relationship::NpcCourtshipOutcome::Informal => {
                NpcPolicyDecisionOutcome::RomanceEstablishedInformal
            }
            crate::relationship::NpcCourtshipOutcome::Ineligible => continue,
        };
        record_decision(
            ctx,
            character_id,
            day,
            NpcPolicyDecisionPhase::Romance,
            receipt_outcome,
            Some(candidate.character_id),
            minute,
        );
        return Ok(());
    }
    record_decision(
        ctx,
        character_id,
        day,
        NpcPolicyDecisionPhase::Romance,
        NpcPolicyDecisionOutcome::RomanceNoCandidate,
        None,
        minute,
    );
    Ok(())
}

/// Formal eligibility is directional even though the autonomous candidate
/// encounter is not. Normalize an opposite-sex pair so scheduler character
/// order cannot turn a father-approved relationship informal. Same-sex and
/// otherwise non-formal pairs retain the deterministic actor/candidate order.
fn npc_courtship_roles(
    actor_id: u64,
    actor_sex: Sex,
    candidate_id: u64,
    candidate_sex: Sex,
) -> (u64, u64) {
    match (actor_sex, candidate_sex) {
        (Sex::Female, Sex::Male) => (candidate_id, actor_id),
        _ => (actor_id, candidate_id),
    }
}

fn process_policy_decisions(ctx: &ReducerContext, time: &CharacterTime) -> Result<(), String> {
    let policy = ctx
        .db
        .npc_policy()
        .character_id()
        .find(time.character_id)
        .ok_or("NPC causal cohort contains a character without policy")?;
    initialize_saved_schedule_once(ctx, time.character_id, policy.policy_seed, time.minutes)?;
    settle_housing_decision(
        ctx,
        time.character_id,
        &policy.home_settlement_id,
        time.minutes,
    )?;
    settle_romance_decision(ctx, time.character_id, policy.policy_seed, time.minutes)?;
    Ok(())
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

    crate::relationship::settle_due_lifecycle_events_global(
        ctx,
        official_minute,
        MAX_LIFECYCLE_EVENTS_PER_CAUSAL_TICK,
    )?;

    let cohort = stable_npc_cohort(ctx, official_minute);
    // The cohort observes one exact personal-time frontier. All durable policy
    // choices are resolved before any member advances, preventing a low id
    // from gaining an extra day of state that changes a peer's same-frontier
    // choice.
    for time in &cohort {
        process_policy_decisions(ctx, time)?;
    }
    for time in cohort {
        let target = official_minute.min(time.minutes.saturating_add(MINUTES_PER_DAY));
        crate::time::advance_stationary_character_to(ctx, time.character_id, target)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::personality::Sex;

    use super::npc_courtship_roles;

    #[test]
    fn scheduler_has_hard_bounded_batches_and_one_day_steps() {
        let source = include_str!("npc_causal.rs");
        assert!(source.contains("truncate(MAX_NPCS_PER_CAUSAL_TICK)"));
        assert!(source.contains("MAX_LIFECYCLE_EVENTS_PER_CAUSAL_TICK"));
        assert!(source.contains("time.minutes.saturating_add(MINUTES_PER_DAY)"));
    }

    #[test]
    fn stable_policy_order_is_frontier_then_character() {
        let source = include_str!("npc_causal.rs");
        assert!(source.contains("sort_by_key(|time| (time.minutes, time.character_id))"));
        assert!(source.contains("retain(|time| time.minutes == frontier)"));
        assert!(source.contains("person.alive && time.minutes < target_minute"));
    }

    #[test]
    fn whole_cohort_decides_before_any_member_advances() {
        let source = include_str!("npc_causal.rs");
        let reducer = source
            .split("pub fn run_npc_causal_tick")
            .nth(1)
            .expect("scheduled reducer");
        let decisions = reducer.find("process_policy_decisions").unwrap();
        let advancement = reducer.find("advance_stationary_character_to").unwrap();
        assert!(decisions < advancement);
        assert!(reducer.contains("for time in &cohort"));
        assert!(reducer.contains("for time in cohort"));
    }

    #[test]
    fn schedule_and_housing_are_receipted_and_scheduler_only() {
        let source = include_str!("npc_causal.rs");
        assert!(source.contains("NpcPolicyDecisionReceipt"));
        assert!(source.contains("ScheduleInitialized"));
        assert!(source.contains("SchedulePreserved"));
        assert!(source.contains("settle_npc_residence"));
        assert!(source.contains("phase_already_decided"));
    }

    #[test]
    fn dependent_schedule_waits_for_authoritative_adulthood() {
        let source = include_str!("npc_causal.rs");
        let initialization = source
            .split("fn initialize_saved_schedule_once")
            .nth(1)
            .expect("schedule initialization")
            .split("fn settle_housing_decision")
            .next()
            .unwrap();
        assert!(initialization.contains("effective_age_years(ctx, character_id, minute)"));
        assert!(initialization.contains("age_years < ADULT_AGE_YEARS"));
        assert!(initialization.contains("NpcPolicyDecisionOutcome::Ineligible"));
        assert!(initialization.contains("has_initialized_schedule_decision"));
        assert!(!initialization.contains("has_prior_schedule_decision"));
    }

    #[test]
    fn romance_is_bounded_to_present_npc_policy_candidates() {
        let source = include_str!("npc_causal.rs");
        let romance = source
            .split("fn settle_romance_decision")
            .nth(1)
            .expect("romance phase")
            .split("fn process_policy_decisions")
            .next()
            .unwrap();
        assert!(romance.contains("settlement_resident_presence"));
        assert!(romance.contains("npc_is_present"));
        assert!(romance.contains("npc_policy()"));
        assert!(romance.contains("stable_candidate_order"));
        assert!(romance.contains("establish_npc_courtship_and_wedding"));
    }

    #[test]
    fn heterosexual_courtship_roles_are_independent_of_actor_order() {
        assert_eq!(
            npc_courtship_roles(10, Sex::Male, 20, Sex::Female),
            (10, 20)
        );
        assert_eq!(
            npc_courtship_roles(20, Sex::Female, 10, Sex::Male),
            (10, 20)
        );
    }

    #[test]
    fn informal_pair_roles_preserve_deterministic_actor_order() {
        assert_eq!(
            npc_courtship_roles(20, Sex::Female, 10, Sex::Female),
            (20, 10)
        );
        assert_eq!(npc_courtship_roles(20, Sex::Male, 10, Sex::Male), (20, 10));
    }

    #[test]
    fn global_events_run_without_participant_login() {
        let source = include_str!("npc_causal.rs");
        let events = source
            .split("pub fn run_npc_causal_tick")
            .nth(1)
            .expect("scheduled reducer");
        assert!(events.contains("settle_due_lifecycle_events_global"));
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
