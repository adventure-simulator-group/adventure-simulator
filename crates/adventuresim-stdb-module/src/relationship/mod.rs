//! Canonical relationships are deliberately separate from asynchronous social
//! edges.  Affinity and familiarity are soft pairwise state; commitments,
//! kinship, pregnancy, and delayed ceremonies are globally exclusive facts.

use adventuresim_core::courtship::{
    ADULT_AGE_YEARS, CONCEPTION_CHANCE_PER_TEN_THOUSAND, ConceptionQuantumState,
    CourtshipDisposition, CourtshipRejection, CourtshipRejectionCode, FORMAL_COURTSHIP_AFFINITY,
    FORMAL_FATHER_APPROVAL_AFFINITY, GESTATION_MINUTES, LeisureInterval, MinuteSpan,
    SPOUSE_LEISURE_MORALE_SPEC, WEDDING_NOTICE_MINUTES, conception_quantum_plan,
    deterministic_child_seeds, encode_courtship_rejection, informal_affinity_threshold,
    joint_leisure_minutes, refresh_bounded_leisure_morale, select_daily_location_target,
    spouse_leisure_earned_milli, stable_lifecycle_hash, succeeds_daily_trial,
    uncovered_minute_spans,
};
use adventuresim_core::strategic_schedule::{DailySchedule, restorative_leisure_spans};
use adventuresim_core::strategic_time::{MINUTES_PER_DAY, MINUTES_PER_YEAR};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::character::{character, character__view, character_death};
use crate::character_skills;
use crate::condition::morale_event as _;
use crate::continuity::{EstateDispositionStatus, estate_disposition};
use crate::corpse::strategic_corpse;
use crate::personality::{
    Courtship as PersonalityCourtship, Inclination, Presentation, Sex, character_personality,
};
use crate::residence::{ResidenceTransitionKind, residence_holding, residence_transition};
use crate::settlement_population::{npc_is_present, settlement_resident_presence};
use crate::social::{CharacterAffinity, character_affinity};
use crate::strategic::{settlement, strategic_gateway_authority__view};
use crate::time::{character_time, character_time__view};
use std::collections::BTreeSet;

// Assembles ordered relationship owners and coordinates the one global queue
// that dispatches both wedding and birth lifecycle events.
include!("model.rs");
include!("projections.rs");
include!("chronology.rs");
include!("family.rs");
include!("commitments.rs");
include!("marriage.rs");
include!("reproduction.rs");
include!("lifecycle.rs");
include!("socializing.rs");
include!("courtship_discovery.rs");
include!("courtship.rs");

/// Process one chronological queue across every globally due relationship
/// event. Full stable keys are ordered before the bound is applied, and
/// temporarily deferred player events do not consume capacity.
pub fn settle_due_lifecycle_events_global(
    ctx: &ReducerContext,
    now: u64,
    limit: usize,
) -> Result<usize, String> {
    let mut due: Vec<_> = ctx
        .db
        .exclusive_commitment()
        .effective_minute()
        .filter(..=now)
        .filter(|row| {
            row.status == CommitmentStatus::Reserved && row.kind == CommitmentKind::Engagement
        })
        .map(|row| DueLifecycleEvent::Wedding {
            effective_minute: row.effective_minute,
            id: row.id,
            participant_id: row.first_character_id,
        })
        .chain(
            ctx.db
                .pregnancy()
                .due_minute()
                .filter(..=now)
                .filter(|row| row.status == PregnancyStatus::Active)
                .map(|row| DueLifecycleEvent::Birth {
                    effective_minute: row.due_minute,
                    id: row.id,
                    mother_id: row.mother_id,
                }),
        )
        .collect();
    due.sort_by(|left, right| left.stable_key().cmp(&right.stable_key()));
    due.retain(|event| event.processable(ctx));
    due.truncate(limit);
    let count = due.len();
    for event in due {
        match event {
            DueLifecycleEvent::Wedding {
                effective_minute,
                id,
                participant_id,
            } => {
                if let Err(error) = settle_due_weddings(ctx, participant_id, effective_minute) {
                    if let Some(commitment) = ctx.db.exclusive_commitment().id().find(&id) {
                        transition_commitment_terminal(
                            ctx,
                            commitment,
                            CommitmentStatus::Cancelled,
                            CommitmentTerminalReason::ResidenceUnavailable,
                            effective_minute,
                        )?;
                    }
                    record_lifecycle_failure(
                        ctx,
                        LifecycleEventKind::Wedding,
                        &id,
                        effective_minute,
                        now,
                        error,
                    );
                }
            }
            DueLifecycleEvent::Birth {
                effective_minute,
                id,
                mother_id,
            } => {
                let pregnancy = ctx.db.pregnancy().id().find(&id);
                if let Some(error) = pregnancy
                    .as_ref()
                    .and_then(|pregnancy| validate_due_birth(ctx, pregnancy).err())
                {
                    quarantine_invalid_birth(ctx, &id, effective_minute);
                    record_lifecycle_failure(
                        ctx,
                        LifecycleEventKind::Birth,
                        &id,
                        effective_minute,
                        now,
                        error,
                    );
                } else {
                    // Preflight precedes every write. An unexpected failure in
                    // the commit path aborts this reducer transaction instead
                    // of being caught after partial character construction.
                    settle_due_births(ctx, mother_id, effective_minute)?;
                }
            }
        }
    }
    Ok(count)
}

#[cfg(test)]
pub(crate) const RELATIONSHIP_SOURCE: &str = concat!(
    include_str!("model.rs"),
    include_str!("projections.rs"),
    include_str!("chronology.rs"),
    include_str!("family.rs"),
    include_str!("commitments.rs"),
    include_str!("marriage.rs"),
    include_str!("reproduction.rs"),
    include_str!("lifecycle.rs"),
    include_str!("socializing.rs"),
    include_str!("courtship_discovery.rs"),
    include_str!("courtship.rs"),
    include_str!("mod.rs"),
);

#[cfg(test)]
mod tests {
    use super::*;

    mod chronology {
        include!("tests/chronology.rs");
    }

    mod projections {
        include!("tests/projections.rs");
    }

    mod family {
        include!("tests/family.rs");
    }

    mod commitments {
        use super::*;
        include!("tests/commitments.rs");
    }

    mod marriage {
        include!("tests/marriage.rs");
    }

    mod reproduction {
        include!("tests/reproduction.rs");
    }

    mod lifecycle {
        use super::*;
        include!("tests/lifecycle.rs");
    }

    mod socializing {
        include!("tests/socializing.rs");
    }

    mod courtship_discovery {
        include!("tests/courtship_discovery.rs");
    }

    mod courtship {
        include!("tests/courtship.rs");
    }
}
