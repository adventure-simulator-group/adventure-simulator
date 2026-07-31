//! Canonical relationships are deliberately separate from asynchronous social
//! edges.  Affinity and familiarity are soft pairwise state; commitments,
//! kinship, pregnancy, and delayed ceremonies are globally exclusive facts.

use adventuresim_core::courtship::{
    ADULT_AGE_YEARS, CONCEPTION_CHANCE_PER_TEN_THOUSAND, ConceptionQuantumState,
    CourtshipDisposition, FORMAL_COURTSHIP_AFFINITY, FORMAL_FATHER_APPROVAL_AFFINITY,
    GESTATION_MINUTES, LeisureInterval, MinuteSpan, SPOUSE_LEISURE_MORALE_SPEC,
    WEDDING_NOTICE_MINUTES, conception_quantum_plan, deterministic_child_seeds,
    informal_affinity_threshold, joint_leisure_minutes, refresh_bounded_leisure_morale,
    select_daily_location_target, spouse_leisure_earned_milli, stable_lifecycle_hash,
    succeeds_daily_trial, uncovered_minute_spans,
};
use adventuresim_core::strategic_schedule::{DailySchedule, restorative_leisure_spans};
use adventuresim_core::strategic_time::{MINUTES_PER_DAY, MINUTES_PER_YEAR};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::character::{character, character__view};
use crate::character_skills;
use crate::condition::morale_event as _;
use crate::corpse::strategic_corpse;
use crate::personality::{
    Courtship as PersonalityCourtship, Inclination, Presentation, Sex, character_personality,
};
use crate::residence::{
    ResidenceTransitionKind, character_residence, residence_occupant, residence_transition,
};
use crate::social::{CharacterAffinity, character_affinity};
use crate::strategic::{settlement, strategic_gateway_authority__view};
use crate::time::{character_time, character_time__view};
use std::collections::BTreeSet;

/// Marks a normal full Character as being advanced by deterministic NPC policy
/// rather than by an account owner.  It intentionally does not weaken the
/// ordinary Character data model or expose private personality data.
#[derive(Clone, Debug)]
#[table(accessor = npc_policy)]
pub struct NpcPolicy {
    #[primary_key]
    pub character_id: u64,
    pub home_settlement_id: String,
    pub policy_seed: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum TemporalScope {
    ActorLocal,
    PairwiseSoft,
    Institutional,
    NpcCanonical,
    ExclusiveShared,
}

/// Enforce the chronology contract at canonical mutation boundaries.
/// Pairwise-soft and institutional interactions intentionally inspect only the
/// actor's frontier: dialogue, affinity, guild, trade, rest, and socializing
/// may address someone without advancing or even reading the target's clock.
pub fn enforce_temporal_scope(
    ctx: &ReducerContext,
    actor_id: u64,
    target_id: Option<u64>,
    scope: TemporalScope,
) -> Result<u64, String> {
    let actor_minute = canonical_now(ctx, actor_id)?;
    match scope {
        TemporalScope::ActorLocal | TemporalScope::PairwiseSoft | TemporalScope::Institutional => {
            Ok(actor_minute)
        }
        TemporalScope::NpcCanonical => {
            let target_id = target_id.ok_or("NPC-canonical scope requires a target")?;
            if ctx.db.npc_policy().character_id().find(target_id).is_none() {
                return Err("NPC-canonical scope requires an NPC-policy character".into());
            }
            Ok(actor_minute)
        }
        TemporalScope::ExclusiveShared => {
            let target_id = target_id.ok_or("Exclusive scope requires a second participant")?;
            let target_minute = canonical_now(ctx, target_id)?;
            // A hard shared fact is effective at the later known frontier. It
            // never rewrites the lagging participant's personal clock. Active
            // uniqueness rows reserve both people immediately, so an actor at
            // an earlier date sees a future engagement as romantically
            // unavailable without learning its private details.
            Ok(actor_minute.max(target_minute))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum KinshipKind {
    Parent,
    Child,
    Sibling,
    Spouse,
}

/// Directed edges; callers create both directions where a relationship needs
/// two readable forms (for example Parent and Child).
#[derive(Clone, Debug)]
#[table(accessor = character_kinship)]
pub struct CharacterKinship {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub subject_id: u64,
    #[index(btree)]
    pub related_id: u64,
    pub kind: KinshipKind,
    pub established_minute: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = household)]
pub struct Household {
    #[primary_key]
    pub id: String,
    pub home_settlement_id: String,
    pub created_minute: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = household_member)]
pub struct HouseholdMember {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub household_id: String,
    /// A character has one authoritative active household at a time.
    #[unique]
    pub character_id: u64,
    pub joined_minute: u64,
    pub role: HouseholdRole,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum HouseholdRole {
    Head,
    Spouse,
    AdultChild,
    Dependent,
}

/// Effective birth coordinate for age derivation. Existing adults are given a
/// synthetic birth minute from their initial age; newborns use the actual
/// delivery minute.
#[derive(Clone, Debug)]
#[table(accessor = character_birth)]
pub struct CharacterBirth {
    #[primary_key]
    pub character_id: u64,
    pub birth_minute: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum CommitmentKind {
    Engagement,
    Marriage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum CommitmentStatus {
    Reserved,
    Fulfilled,
    Cancelled,
    Expired,
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum CommitmentTerminalReason {
    WeddingCompleted,
    ParticipantDead,
    ParticipantUnderage,
    CeremonyLocationUnavailable,
    ResidenceUnavailable,
    CancelledByParticipant,
    ReservationExpired,
    MarriageEnded,
}

/// One row owns the pair and one uniqueness row owns each participant.  This
/// lets an atomic reducer reject a competing romantic claim before it writes
/// any history.
#[derive(Clone, Debug)]
#[table(accessor = exclusive_commitment)]
pub struct ExclusiveCommitment {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub first_character_id: u64,
    #[index(btree)]
    pub second_character_id: u64,
    pub kind: CommitmentKind,
    pub status: CommitmentStatus,
    pub ceremony_settlement_id: String,
    #[index(btree)]
    pub effective_minute: u64,
    pub created_minute: u64,
    pub resolved_minute: Option<u64>,
    pub terminal_reason: Option<CommitmentTerminalReason>,
}

#[derive(Clone, Debug)]
#[table(accessor = exclusive_commitment_participant)]
pub struct ExclusiveCommitmentParticipant {
    #[primary_key]
    pub character_id: u64,
    pub commitment_id: String,
}

#[derive(Clone, Debug)]
#[table(accessor = commitment_event)]
pub struct CommitmentEvent {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub commitment_id: String,
    pub status: CommitmentStatus,
    pub reason: Option<CommitmentTerminalReason>,
    pub minute: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum MarriageStatus {
    Active,
    Widowed,
    Ended,
}

#[derive(Clone, Debug)]
#[table(accessor = marriage)]
pub struct Marriage {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub first_character_id: u64,
    #[index(btree)]
    pub second_character_id: u64,
    pub commitment_id: String,
    pub household_id: String,
    pub ceremony_settlement_id: String,
    pub married_minute: u64,
    pub status: MarriageStatus,
    pub resolved_minute: Option<u64>,
}

#[derive(Clone, Debug)]
#[table(accessor = marriage_participant)]
pub struct MarriageParticipant {
    #[primary_key]
    pub character_id: u64,
    pub marriage_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum CourtshipKind {
    Formal,
    Informal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum CourtshipStatus {
    Active,
    Exposed,
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum CourtshipSecrecyReason {
    FatherDisapproval,
    FormalRouteUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum CourtshipTerminalReason {
    EngagementScheduled,
    EndedByParticipant,
    PartnerUnavailable,
}

#[derive(Clone, Debug)]
#[table(accessor = courtship)]
pub struct CourtshipRecord {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub first_character_id: u64,
    #[index(btree)]
    pub second_character_id: u64,
    pub kind: CourtshipKind,
    pub status: CourtshipStatus,
    pub secrecy_reason: Option<CourtshipSecrecyReason>,
    pub started_minute: u64,
    /// First relationship-day whose observer checks have not been resolved.
    pub next_discovery_day: u64,
    pub resolved_minute: Option<u64>,
    pub terminal_reason: Option<CourtshipTerminalReason>,
}

/// Immutable receipt for every attempt, successful or not. The day is in the
/// primary key, enforcing at most one check per observer and relationship day.
#[derive(Clone, Debug)]
#[table(accessor = courtship_discovery)]
pub struct CourtshipDiscovery {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub courtship_id: String,
    #[index(btree)]
    pub observer_id: u64,
    pub day: u64,
    pub attempted_minute: u64,
    pub succeeded: bool,
    pub observer_insight: f32,
    pub weaker_deception: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum PregnancyStatus {
    Active,
    Born,
    Ended,
}

#[derive(Clone, Debug)]
#[table(accessor = pregnancy)]
pub struct Pregnancy {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub mother_id: u64,
    #[index(btree)]
    pub father_id: u64,
    pub ordinal: u64,
    pub conceived_minute: u64,
    #[index(btree)]
    pub due_minute: u64,
    pub reserved_child_id: u64,
    pub child_name_seed: u64,
    pub child_female: bool,
    pub child_home_seed: u64,
    pub birth_settlement_id: String,
    pub birth_residence_character_id: Option<u64>,
    pub status: PregnancyStatus,
    pub birth_character_id: Option<u64>,
    pub resolved_minute: Option<u64>,
}

#[derive(Clone, Debug)]
#[table(accessor = active_pregnancy)]
pub struct ActivePregnancy {
    #[primary_key]
    pub mother_id: u64,
    pub pregnancy_id: String,
}

/// Durable reservation prevents any ordinary character-creation path from
/// claiming an identity already promised to a pregnancy.
#[derive(Clone, Debug)]
#[table(accessor = child_identity_reservation)]
pub struct ChildIdentityReservation {
    #[primary_key]
    pub character_id: u64,
    pub pregnancy_id: String,
    pub reserved_minute: u64,
}

/// A realized, same-location Leisure span. Spouses write their own spans; a
/// canonical pair processor intersects them without consuming either clock.
#[derive(Clone, Debug)]
#[table(accessor = spouse_leisure_slice)]
pub struct SpouseLeisureSlice {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    pub start_minute: u64,
    pub end_minute: u64,
    pub location_id: String,
}

#[derive(Clone, Debug)]
#[table(accessor = spouse_leisure_overlap)]
pub struct SpouseLeisureOverlap {
    #[primary_key]
    pub id: String,
    pub first_slice_id: String,
    pub second_slice_id: String,
    pub joint_minutes: u64,
    pub resolved_minute: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = spouse_leisure_accrual)]
pub struct SpouseLeisureAccrual {
    #[primary_key]
    pub pair_id: String,
    pub first_character_id: u64,
    pub second_character_id: u64,
    pub conserved_joint_minutes: u8,
    pub next_trial_ordinal: u64,
    pub total_joint_minutes: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = conception_trial_receipt)]
pub struct ConceptionTrialReceipt {
    #[primary_key]
    pub id: String,
    pub pair_id: String,
    pub ordinal: u64,
    pub minute: u64,
    pub succeeded: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum DowryOutcomeKind {
    Paid,
    FatherUnavailable,
    NoDowry,
    InsufficientFunds,
    NotFormal,
}

#[derive(Clone, Debug)]
#[table(accessor = dowry_outcome)]
pub struct DowryOutcome {
    #[primary_key]
    pub commitment_id: String,
    pub father_id: Option<u64>,
    pub recipient_id: u64,
    pub amount: u32,
    pub outcome: DowryOutcomeKind,
    pub minute: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum LifecycleEventKind {
    Wedding,
    Birth,
}

/// Durable evidence that a malformed gameplay event was quarantined rather
/// than poisoning the private recurring scheduler. Infrastructure failures
/// still abort the scheduler reducer and are not recorded here.
#[derive(Clone, Debug)]
#[table(accessor = lifecycle_event_failure)]
pub struct LifecycleEventFailure {
    #[primary_key]
    pub id: String,
    pub event_kind: LifecycleEventKind,
    pub event_id: String,
    pub effective_minute: u64,
    pub recorded_minute: u64,
    pub error: String,
}

/// A deliberately actor-scoped summary for the trusted strategic gateway.
/// The underlying relationship, kinship, commitment, and pregnancy tables
/// remain private: the gateway filters this projection to the signed-in
/// character before presenting it to the browser.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendCharacterRelationshipStatus {
    pub character_id: u64,
    pub spouse_id: Option<u64>,
    pub courtship_partner_id: Option<u64>,
    pub courtship_kind: Option<String>,
    pub courtship_exposed: bool,
    pub pregnancy_due_minute: Option<u64>,
    pub pregnancy_child_id: Option<u64>,
}

/// Observer-scoped knowledge of a discovered facade. The gateway may return a
/// row only to `observer_character_id`; partners and unrelated characters do
/// not learn which family member discovered the relationship.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendCourtshipDiscoveryStatus {
    pub observer_character_id: u64,
    pub first_character_id: u64,
    pub second_character_id: u64,
    pub discovered_minute: u64,
}

fn is_strategic_gateway(ctx: &ViewContext) -> bool {
    ctx.db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|authority| authority.identity == ctx.sender())
}

/// Do not make the private relationship tables public merely for UI work.
/// The web gateway is the trust boundary and asks for the active character's
/// one-row summary; direct clients receive no rows at all.
#[view(accessor = backend_character_relationship_statuses, public)]
pub fn backend_character_relationship_statuses(
    ctx: &ViewContext,
) -> Vec<BackendCharacterRelationshipStatus> {
    if !is_strategic_gateway(ctx) {
        return Vec::new();
    }
    let mut character_ids = BTreeSet::new();
    for edge in ctx.db.character_kinship().subject_id().filter(0u64..) {
        character_ids.insert(edge.subject_id);
        character_ids.insert(edge.related_id);
    }
    for courtship in ctx.db.courtship().first_character_id().filter(0u64..) {
        character_ids.insert(courtship.first_character_id);
        character_ids.insert(courtship.second_character_id);
    }
    for pregnancy in ctx.db.pregnancy().father_id().filter(0u64..) {
        character_ids.insert(pregnancy.mother_id);
        character_ids.insert(pregnancy.father_id);
    }
    character_ids
        .into_iter()
        .filter_map(|character_id| {
            ctx.db.character().id().find(character_id).map(|character| {
                let observer_minute = ctx
                    .db
                    .character_time()
                    .character_id()
                    .find(character.id)
                    .map_or(0, |time| time.minutes);
                let spouse_id = ctx
                    .db
                    .marriage()
                    .first_character_id()
                    .filter(character.id)
                    .chain(ctx.db.marriage().second_character_id().filter(character.id))
                    .find(|marriage| {
                        (marriage.first_character_id == character.id
                            || marriage.second_character_id == character.id)
                            && marriage.married_minute <= observer_minute
                            && marriage
                                .resolved_minute
                                .is_none_or(|resolved| resolved > observer_minute)
                    })
                    .map(|marriage| {
                        if marriage.first_character_id == character.id {
                            marriage.second_character_id
                        } else {
                            marriage.first_character_id
                        }
                    });
                let courtship = ctx
                    .db
                    .courtship()
                    .first_character_id()
                    .filter(character.id)
                    .find(|row| {
                        row.started_minute <= observer_minute
                            && row
                                .resolved_minute
                                .is_none_or(|resolved| resolved > observer_minute)
                    })
                    .or_else(|| {
                        ctx.db
                            .courtship()
                            .second_character_id()
                            .filter(character.id)
                            .find(|row| {
                                row.started_minute <= observer_minute
                                    && row
                                        .resolved_minute
                                        .is_none_or(|resolved| resolved > observer_minute)
                            })
                    });
                let (courtship_partner_id, courtship_kind, courtship_exposed) =
                    courtship.map_or((None, None, false), |row| {
                        let exposed = ctx
                            .db
                            .courtship_discovery()
                            .courtship_id()
                            .filter(&row.id)
                            .any(|receipt| {
                                receipt.succeeded && receipt.attempted_minute <= observer_minute
                            });
                        (
                            Some(if row.first_character_id == character.id {
                                row.second_character_id
                            } else {
                                row.first_character_id
                            }),
                            Some(
                                match row.kind {
                                    CourtshipKind::Formal => "formal",
                                    CourtshipKind::Informal => "informal",
                                }
                                .into(),
                            ),
                            exposed,
                        )
                    });
                let active_pregnancy = ctx
                    .db
                    .pregnancy()
                    .mother_id()
                    .filter(character.id)
                    .chain(ctx.db.pregnancy().father_id().filter(character.id))
                    .find(|row| {
                        (row.mother_id == character.id || row.father_id == character.id)
                            && row.status == PregnancyStatus::Active
                            && row.conceived_minute <= observer_minute
                    });
                let born_child_id = ctx
                    .db
                    .pregnancy()
                    .mother_id()
                    .filter(character.id)
                    .chain(ctx.db.pregnancy().father_id().filter(character.id))
                    .filter(|row| {
                        (row.mother_id == character.id || row.father_id == character.id)
                            && row.status == PregnancyStatus::Born
                            && row.due_minute <= observer_minute
                    })
                    .max_by_key(|row| (row.due_minute, row.id.clone()))
                    .and_then(|row| row.birth_character_id);
                BackendCharacterRelationshipStatus {
                    character_id: character.id,
                    spouse_id,
                    courtship_partner_id,
                    courtship_kind,
                    courtship_exposed,
                    pregnancy_due_minute: active_pregnancy.map(|row| row.due_minute),
                    pregnancy_child_id: born_child_id,
                }
            })
        })
        .collect()
}

#[view(accessor = backend_courtship_discoveries, public)]
pub fn backend_courtship_discoveries(ctx: &ViewContext) -> Vec<BackendCourtshipDiscoveryStatus> {
    if !is_strategic_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .courtship_discovery()
        .observer_id()
        .filter(0u64..)
        .filter(|receipt| receipt.succeeded)
        .filter_map(|receipt| {
            let observer_minute = ctx
                .db
                .character_time()
                .character_id()
                .find(receipt.observer_id)
                .map_or(0, |time| time.minutes);
            (receipt.attempted_minute <= observer_minute)
                .then(|| ctx.db.courtship().id().find(&receipt.courtship_id))
                .flatten()
                .map(|courtship| BackendCourtshipDiscoveryStatus {
                    observer_character_id: receipt.observer_id,
                    first_character_id: courtship.first_character_id,
                    second_character_id: courtship.second_character_id,
                    discovered_minute: receipt.attempted_minute,
                })
        })
        .collect()
}

/// A receipt applies a particular chronological slice only once.  Its target
/// is selected from the day rather than the advancement chunk, so long and
/// short time advances retain the same socializing partner.
#[derive(Clone, Debug)]
#[table(accessor = socializing_receipt)]
pub struct SocializingReceipt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub actor_id: u64,
    #[index(btree)]
    pub target_id: u64,
    pub start_minute: u64,
    pub end_minute: u64,
    pub minutes: u64,
}

pub fn canonical_now(ctx: &ReducerContext, character_id: u64) -> Result<u64, String> {
    ctx.db
        .character_time()
        .character_id()
        .find(character_id)
        .map(|time| time.minutes)
        .ok_or_else(|| "Character time record not found".to_string())
}

/// Earliest relationship boundary which can change the meaning of an actor's
/// interval. Global materialization may already have written a future fact;
/// callers must still split the actor's personal interval at its effective
/// minute before applying leisure, household, or spouse consequences.
pub(crate) fn next_lifecycle_boundary(
    ctx: &ReducerContext,
    character_id: u64,
    start_minute: u64,
    end_minute: u64,
) -> Option<u64> {
    let birthday = ctx
        .db
        .character_birth()
        .character_id()
        .find(character_id)
        .and_then(|birth| {
            let year = i128::from(MINUTES_PER_YEAR);
            let start = i128::from(start_minute);
            let birth_minute = i128::from(birth.birth_minute);
            let completed = (start.saturating_sub(birth_minute)).max(0) / year;
            let next = birth_minute.saturating_add((completed + 1).saturating_mul(year));
            u64::try_from(next)
                .ok()
                .filter(|minute| start_minute < *minute && *minute < end_minute)
        });
    let wedding = ctx
        .db
        .exclusive_commitment()
        .effective_minute()
        .filter((start_minute.saturating_add(1))..end_minute)
        .filter(|row| {
            row.status == CommitmentStatus::Reserved
                && row.kind == CommitmentKind::Engagement
                && (row.first_character_id == character_id
                    || row.second_character_id == character_id)
        })
        .map(|row| row.effective_minute)
        .next();
    let birth = ctx
        .db
        .pregnancy()
        .due_minute()
        .filter((start_minute.saturating_add(1))..end_minute)
        .filter(|row| {
            row.status == PregnancyStatus::Active
                && (row.mother_id == character_id || row.father_id == character_id)
        })
        .map(|row| row.due_minute)
        .next();
    let marriage = ctx
        .db
        .marriage()
        .iter()
        .filter(|row| {
            row.first_character_id == character_id || row.second_character_id == character_id
        })
        .flat_map(|row| [Some(row.married_minute), row.resolved_minute])
        .flatten()
        .filter(|minute| start_minute < *minute && *minute < end_minute)
        .min();
    wedding
        .into_iter()
        .chain(birth)
        .chain(marriage)
        .chain(birthday)
        .min()
}

pub fn initialize_npc_policy(
    ctx: &ReducerContext,
    character_id: u64,
    home_settlement_id: String,
    policy_seed: u64,
) -> Result<(), String> {
    if ctx.db.character().id().find(character_id).is_none() {
        return Err("NPC policy requires a full Character".into());
    }
    crate::character::validate_full_character_components(ctx, character_id)?;
    if ctx
        .db
        .npc_policy()
        .character_id()
        .find(character_id)
        .is_none()
    {
        ctx.db.npc_policy().insert(NpcPolicy {
            character_id,
            home_settlement_id,
            policy_seed,
        });
    }
    Ok(())
}

/// A deliberately narrow advancement primitive. NPC policies use it to move
/// their ordinary CharacterTime and settle due canonical events atomically;
/// player travel, schedules, health, and account authority never run through
/// this path.
pub fn advance_npc_personal_time(
    ctx: &ReducerContext,
    character_id: u64,
    target_minute: u64,
) -> Result<(), String> {
    if ctx
        .db
        .npc_policy()
        .character_id()
        .find(character_id)
        .is_none()
    {
        return Err("Character is not NPC-policy controlled".into());
    }
    let mut time = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or("NPC CharacterTime record not found")?;
    if target_minute < time.minutes {
        return Err("Canonical NPC time cannot be written retroactively".into());
    }
    if let Some(boundary) = next_lifecycle_boundary(ctx, character_id, time.minutes, target_minute)
    {
        advance_npc_personal_time(ctx, character_id, boundary)?;
        return advance_npc_personal_time(ctx, character_id, target_minute);
    }
    // Delayed events are settled inside this transaction at the target
    // frontier. Any error rolls back both their effects and the clock, so an
    // NPC can never skip a due event by retaining an advanced date.
    time.minutes = target_minute;
    ctx.db.character_time().character_id().update(time);
    crate::time::settle_lifecycle_after_character_time_write(ctx, character_id, target_minute)
}

fn canonical_pair(first: u64, second: u64) -> (u64, u64) {
    if first < second {
        (first, second)
    } else {
        (second, first)
    }
}

fn commitment_id(first: u64, second: u64) -> String {
    let (first, second) = canonical_pair(first, second);
    format!("commitment:{first}:{second}")
}

fn record_commitment_event(
    ctx: &ReducerContext,
    commitment: &ExclusiveCommitment,
    status: CommitmentStatus,
    reason: Option<CommitmentTerminalReason>,
    minute: u64,
) {
    let id = format!("commitment-event:{}:{minute}:{status:?}", commitment.id);
    if ctx.db.commitment_event().id().find(&id).is_none() {
        ctx.db.commitment_event().insert(CommitmentEvent {
            id,
            commitment_id: commitment.id.clone(),
            status,
            reason,
            minute,
        });
    }
}

/// Resolve a reservation exactly once and release both active uniqueness rows
/// in the same transaction on every terminal path.
fn transition_commitment_terminal(
    ctx: &ReducerContext,
    mut commitment: ExclusiveCommitment,
    status: CommitmentStatus,
    reason: CommitmentTerminalReason,
    minute: u64,
) -> ExclusiveCommitment {
    if commitment.status != CommitmentStatus::Reserved {
        return commitment;
    }
    for character_id in [
        commitment.first_character_id,
        commitment.second_character_id,
    ] {
        if ctx
            .db
            .exclusive_commitment_participant()
            .character_id()
            .find(character_id)
            .is_some_and(|row| row.commitment_id == commitment.id)
        {
            ctx.db
                .exclusive_commitment_participant()
                .character_id()
                .delete(character_id);
        }
    }
    commitment.status = status;
    commitment.resolved_minute = Some(minute);
    commitment.terminal_reason = Some(reason);
    ctx.db
        .exclusive_commitment()
        .id()
        .update(commitment.clone());
    record_commitment_event(ctx, &commitment, status, Some(reason), minute);
    commitment
}

/// Reserve two people now and schedule their marriage a year later.  The
/// scheduling transaction has no player-clock write and therefore remains a
/// canonical exclusive event even when ordinary social edges are asynchronous.
pub fn reserve_wedding(
    ctx: &ReducerContext,
    first_character_id: u64,
    second_character_id: u64,
    scheduled_from_minute: u64,
) -> Result<ExclusiveCommitment, String> {
    if first_character_id == second_character_id {
        return Err("A character cannot marry themself".into());
    }
    let (first, second) = canonical_pair(first_character_id, second_character_id);
    for participant in [first, second] {
        if let Some(existing) = ctx
            .db
            .exclusive_commitment_participant()
            .character_id()
            .find(participant)
        {
            return Err(format!(
                "Character already has exclusive commitment {}",
                existing.commitment_id
            ));
        }
        if let Some(existing) = ctx
            .db
            .marriage_participant()
            .character_id()
            .find(participant)
        {
            return Err(format!(
                "Character is already in active marriage {}",
                existing.marriage_id
            ));
        }
    }
    let first_person = ctx
        .db
        .character()
        .id()
        .find(first)
        .ok_or("Engaged character not found")?;
    let second_person = ctx
        .db
        .character()
        .id()
        .find(second)
        .ok_or("Engaged character not found")?;
    let ceremony_settlement_id = first_person
        .current_settlement_id
        .filter(|settlement| second_person.current_settlement_id.as_ref() == Some(settlement))
        .ok_or("Wedding scheduling requires a shared ceremony settlement")?;
    let prefix = commitment_id(first, second);
    let ordinal = ctx
        .db
        .exclusive_commitment()
        .first_character_id()
        .filter(first)
        .filter(|row| row.second_character_id == second)
        .count();
    let id = format!("{prefix}:{scheduled_from_minute}:{ordinal}");
    let row = ExclusiveCommitment {
        id: id.clone(),
        first_character_id: first,
        second_character_id: second,
        kind: CommitmentKind::Engagement,
        status: CommitmentStatus::Reserved,
        ceremony_settlement_id,
        effective_minute: scheduled_from_minute.saturating_add(WEDDING_NOTICE_MINUTES),
        created_minute: scheduled_from_minute,
        resolved_minute: None,
        terminal_reason: None,
    };
    ctx.db.exclusive_commitment().insert(row.clone());
    for character_id in [first, second] {
        ctx.db
            .exclusive_commitment_participant()
            .insert(ExclusiveCommitmentParticipant {
                character_id,
                commitment_id: id.clone(),
            });
    }
    record_commitment_event(
        ctx,
        &row,
        CommitmentStatus::Reserved,
        None,
        scheduled_from_minute,
    );
    Ok(row)
}

fn kinship_id(subject_id: u64, related_id: u64, kind: KinshipKind) -> String {
    format!("kinship:{subject_id}:{related_id}:{kind:?}")
}

fn ensure_kinship(
    ctx: &ReducerContext,
    subject_id: u64,
    related_id: u64,
    kind: KinshipKind,
    minute: u64,
) {
    let id = kinship_id(subject_id, related_id, kind);
    if ctx.db.character_kinship().id().find(&id).is_none() {
        ctx.db.character_kinship().insert(CharacterKinship {
            id,
            subject_id,
            related_id,
            kind,
            established_minute: minute,
        });
    }
}

fn leave_household(ctx: &ReducerContext, character_id: u64) {
    if let Some(member) = ctx.db.household_member().character_id().find(character_id) {
        ctx.db.household_member().id().delete(&member.id);
    }
}

fn join_household(
    ctx: &ReducerContext,
    household_id: &str,
    character_id: u64,
    minute: u64,
    role: HouseholdRole,
) {
    if ctx
        .db
        .household_member()
        .character_id()
        .find(character_id)
        .is_some_and(|member| member.household_id == household_id)
    {
        return;
    }
    leave_household(ctx, character_id);
    ctx.db.household_member().insert(HouseholdMember {
        id: format!("household:{household_id}:{character_id}"),
        household_id: household_id.to_owned(),
        character_id,
        joined_minute: minute,
        role,
    });
}

pub fn record_character_birth(ctx: &ReducerContext, character_id: u64, birth_minute: i64) {
    if ctx
        .db
        .character_birth()
        .character_id()
        .find(character_id)
        .is_none()
    {
        ctx.db.character_birth().insert(CharacterBirth {
            character_id,
            birth_minute,
        });
    }
}

pub fn effective_age_years(ctx: &ReducerContext, character_id: u64, minute: u64) -> Option<u16> {
    let character = ctx.db.character().id().find(character_id)?;
    let Some(birth) = ctx.db.character_birth().character_id().find(character_id) else {
        return Some(character.age_years);
    };
    let elapsed = i128::from(minute).saturating_sub(i128::from(birth.birth_minute));
    Some((elapsed.max(0) as u128 / u128::from(MINUTES_PER_YEAR)).min(u128::from(u16::MAX)) as u16)
}

/// Refresh the cached display age from the authoritative birth coordinate.
/// Calling this at every lifecycle boundary naturally promotes dependents at
/// their yearly boundary without granting newborn starter equipment.
pub fn settle_character_age(ctx: &ReducerContext, character_id: u64, minute: u64) {
    let Some(mut character) = ctx.db.character().id().find(character_id) else {
        return;
    };
    let Some(age_years) = effective_age_years(ctx, character_id, minute) else {
        return;
    };
    if character.age_years != age_years {
        character.age_years = age_years;
        ctx.db.character().id().update(character);
    }
}

/// Turn the deterministic resident roster into coherent authoritative family
/// units. Each complete cohort is father, mother, adult daughter, adult son;
/// incomplete tails still receive one household and unique roles, but no
/// fabricated identities or kinship edges.
pub fn ensure_seeded_family_households(
    ctx: &ReducerContext,
    settlement_id: &str,
) -> Result<(), String> {
    let mut residents: Vec<_> = ctx
        .db
        .npc_policy()
        .iter()
        .filter(|policy| policy.home_settlement_id == settlement_id)
        .map(|policy| policy.character_id)
        .collect();
    residents.sort_unstable();
    for (cohort, family) in residents.chunks(4).enumerate() {
        let household_id = format!("household:seeded:{settlement_id}:{cohort}");
        if ctx.db.household().id().find(&household_id).is_none() {
            ctx.db.household().insert(Household {
                id: household_id.clone(),
                home_settlement_id: settlement_id.to_owned(),
                created_minute: 0,
            });
        }
        let roles = [
            HouseholdRole::Head,
            HouseholdRole::Spouse,
            HouseholdRole::AdultChild,
            HouseholdRole::AdultChild,
        ];
        for (index, character_id) in family.iter().copied().enumerate() {
            join_household(ctx, &household_id, character_id, 0, roles[index]);
        }
        if family.len() < 4 {
            continue;
        }
        let assigned = [
            (family[0], Sex::Male, Presentation::Man, 52u16),
            (family[1], Sex::Female, Presentation::Woman, 48u16),
            (family[2], Sex::Female, Presentation::Woman, 24u16),
            (family[3], Sex::Male, Presentation::Man, 21u16),
        ];
        for (character_id, sex, presentation, age) in assigned {
            let mut character = ctx
                .db
                .character()
                .id()
                .find(character_id)
                .ok_or("Seeded family member is missing its Character")?;
            character.age_years = age;
            ctx.db.character().id().update(character);
            let mut personality = ctx
                .db
                .character_personality()
                .character_id()
                .find(character_id)
                .ok_or("Seeded family member is missing personality")?;
            personality.sex = sex;
            personality.presentation = presentation;
            ctx.db
                .character_personality()
                .character_id()
                .update(personality);
            let birth = CharacterBirth {
                character_id,
                birth_minute: -(i64::from(age)
                    * i64::try_from(MINUTES_PER_YEAR).unwrap_or(i64::MAX)),
            };
            if ctx
                .db
                .character_birth()
                .character_id()
                .find(character_id)
                .is_some()
            {
                ctx.db.character_birth().character_id().update(birth);
            } else {
                ctx.db.character_birth().insert(birth);
            }
        }
        for child in [family[2], family[3]] {
            for parent in [family[0], family[1]] {
                ensure_kinship(ctx, child, parent, KinshipKind::Parent, 0);
                ensure_kinship(ctx, parent, child, KinshipKind::Child, 0);
            }
        }
        ensure_kinship(ctx, family[2], family[3], KinshipKind::Sibling, 0);
        ensure_kinship(ctx, family[3], family[2], KinshipKind::Sibling, 0);
    }
    Ok(())
}

fn father_of(ctx: &ReducerContext, child_id: u64) -> Option<u64> {
    ctx.db.character_kinship().iter().find_map(|edge| {
        (edge.subject_id == child_id && edge.kind == KinshipKind::Parent)
            .then(|| {
                ctx.db
                    .character()
                    .id()
                    .find(edge.related_id)
                    .filter(|person| person.alive)
                    .and_then(|_| {
                        ctx.db
                            .character_personality()
                            .character_id()
                            .find(edge.related_id)
                    })
                    .filter(|personality| personality.sex == Sex::Male)
                    .map(|_| edge.related_id)
            })
            .flatten()
    })
}

fn formal_dowry_amount(father_wealth: u64) -> u32 {
    if father_wealth >= 300 {
        100
    } else if father_wealth >= 100 {
        45
    } else if father_wealth >= 30 {
        15
    } else {
        0
    }
}

pub fn settle_due_weddings(
    ctx: &ReducerContext,
    participant_id: u64,
    now: u64,
) -> Result<(), String> {
    let due: Vec<_> = ctx
        .db
        .exclusive_commitment()
        .iter()
        .filter(|row| {
            row.status == CommitmentStatus::Reserved
                && row.kind == CommitmentKind::Engagement
                && row.effective_minute <= now
                && (row.first_character_id == participant_id
                    || row.second_character_id == participant_id)
        })
        .collect();
    for commitment in due {
        let effective_minute = commitment.effective_minute;
        // NPC participants causally reach the ceremony without login. A
        // player-controlled participant is never advanced implicitly; the
        // reservation remains pending until that personal frontier reaches
        // the ceremony, preventing location checks against stale history.
        let mut player_lagging = false;
        for character_id in [
            commitment.first_character_id,
            commitment.second_character_id,
        ] {
            let frontier = canonical_now(ctx, character_id)?;
            if frontier < effective_minute {
                if ctx
                    .db
                    .npc_policy()
                    .character_id()
                    .find(character_id)
                    .is_some()
                {
                    advance_npc_personal_time(ctx, character_id, effective_minute)?;
                } else {
                    player_lagging = true;
                }
            }
        }
        if player_lagging {
            continue;
        }
        let Some(commitment) = ctx.db.exclusive_commitment().id().find(&commitment.id) else {
            continue;
        };
        if commitment.status != CommitmentStatus::Reserved {
            continue;
        }
        let Some(first) = ctx.db.character().id().find(commitment.first_character_id) else {
            transition_commitment_terminal(
                ctx,
                commitment,
                CommitmentStatus::Cancelled,
                CommitmentTerminalReason::ParticipantDead,
                effective_minute,
            );
            continue;
        };
        let Some(second) = ctx.db.character().id().find(commitment.second_character_id) else {
            transition_commitment_terminal(
                ctx,
                commitment,
                CommitmentStatus::Cancelled,
                CommitmentTerminalReason::ParticipantDead,
                effective_minute,
            );
            continue;
        };
        if !first.alive || !second.alive {
            transition_commitment_terminal(
                ctx,
                commitment,
                CommitmentStatus::Cancelled,
                CommitmentTerminalReason::ParticipantDead,
                effective_minute,
            );
            continue;
        }
        if effective_age_years(ctx, first.id, effective_minute).unwrap_or(first.age_years)
            < ADULT_AGE_YEARS
            || effective_age_years(ctx, second.id, effective_minute).unwrap_or(second.age_years)
                < ADULT_AGE_YEARS
        {
            transition_commitment_terminal(
                ctx,
                commitment,
                CommitmentStatus::Cancelled,
                CommitmentTerminalReason::ParticipantUnderage,
                effective_minute,
            );
            continue;
        }
        if first.current_settlement_id.as_deref() != Some(&commitment.ceremony_settlement_id)
            || second.current_settlement_id.as_deref() != Some(&commitment.ceremony_settlement_id)
        {
            transition_commitment_terminal(
                ctx,
                commitment,
                CommitmentStatus::Cancelled,
                CommitmentTerminalReason::CeremonyLocationUnavailable,
                effective_minute,
            );
            continue;
        }
        let residence_character_id = [first.id, second.id].into_iter().find(|character_id| {
            crate::residence::active_primary_residence(
                ctx,
                *character_id,
                &commitment.ceremony_settlement_id,
            )
            .is_some()
        });
        let Some(residence_character_id) = residence_character_id else {
            transition_commitment_terminal(
                ctx,
                commitment,
                CommitmentStatus::Cancelled,
                CommitmentTerminalReason::ResidenceUnavailable,
                effective_minute,
            );
            continue;
        };
        let courtship_id = format!(
            "courtship:{}:{}",
            first.id.min(second.id),
            first.id.max(second.id)
        );
        let formal = ctx
            .db
            .courtship()
            .id()
            .find(&courtship_id)
            .is_some_and(|courtship| courtship.kind == CourtshipKind::Formal);
        let (bride_id, recipient_id) = [first.id, second.id]
            .into_iter()
            .find_map(|candidate| {
                ctx.db
                    .character_personality()
                    .character_id()
                    .find(candidate)
                    .filter(|personality| personality.sex == Sex::Female)
                    .map(|_| {
                        (
                            candidate,
                            if candidate == first.id {
                                second.id
                            } else {
                                first.id
                            },
                        )
                    })
            })
            .unwrap_or((first.id, second.id));
        let planned_dowry = if !formal {
            (None, 0, DowryOutcomeKind::NotFormal)
        } else if let Some(father) = father_of(ctx, bride_id) {
            let amount = formal_dowry_amount(crate::item::personal_currency_total(ctx, father));
            if amount == 0 {
                (Some(father), 0, DowryOutcomeKind::NoDowry)
            } else if crate::item::personal_currency_total(ctx, father) < u64::from(amount) {
                (Some(father), amount, DowryOutcomeKind::InsufficientFunds)
            } else {
                crate::item::validate_personal_currency_credit(
                    ctx,
                    &commitment.ceremony_settlement_id,
                    amount,
                )?;
                (Some(father), amount, DowryOutcomeKind::Paid)
            }
        } else {
            (None, 0, DowryOutcomeKind::FatherUnavailable)
        };
        // All fallible validation is complete before the first durable write.
        if let (Some(father), amount, DowryOutcomeKind::Paid) = planned_dowry {
            crate::item::consume_personal_currency(ctx, father, u64::from(amount))
                .expect("dowry funds were prevalidated");
            crate::item::credit_personal_currency(
                ctx,
                recipient_id,
                &commitment.ceremony_settlement_id,
                amount,
            )
            .expect("dowry currency was prevalidated");
        }
        let household_id = format!("household:{}", commitment.id);
        if ctx.db.household().id().find(&household_id).is_none() {
            ctx.db.household().insert(Household {
                id: household_id.clone(),
                home_settlement_id: commitment.ceremony_settlement_id.clone(),
                created_minute: commitment.effective_minute,
            });
        }
        for (character_id, role) in [
            (first.id, HouseholdRole::Head),
            (second.id, HouseholdRole::Spouse),
        ] {
            join_household(
                ctx,
                &household_id,
                character_id,
                commitment.effective_minute,
                role,
            );
            crate::residence::move_residence_occupant_internal(
                ctx,
                residence_character_id,
                character_id,
                commitment.effective_minute,
            )
            .expect("active wedding residence was prevalidated");
        }
        ensure_kinship(
            ctx,
            first.id,
            second.id,
            KinshipKind::Spouse,
            commitment.effective_minute,
        );
        ensure_kinship(
            ctx,
            second.id,
            first.id,
            KinshipKind::Spouse,
            commitment.effective_minute,
        );
        if ctx
            .db
            .dowry_outcome()
            .commitment_id()
            .find(&commitment.id)
            .is_none()
        {
            let (father_id, amount, outcome) = planned_dowry;
            ctx.db.dowry_outcome().insert(DowryOutcome {
                commitment_id: commitment.id.clone(),
                father_id,
                recipient_id,
                amount,
                outcome,
                minute: commitment.effective_minute,
            });
        }
        let marriage_id = format!("marriage:{}", commitment.id);
        if ctx.db.marriage().id().find(&marriage_id).is_none() {
            ctx.db.marriage().insert(Marriage {
                id: marriage_id.clone(),
                first_character_id: first.id,
                second_character_id: second.id,
                commitment_id: commitment.id.clone(),
                household_id: household_id.clone(),
                ceremony_settlement_id: commitment.ceremony_settlement_id.clone(),
                married_minute: commitment.effective_minute,
                status: MarriageStatus::Active,
                resolved_minute: None,
            });
            for character_id in [first.id, second.id] {
                ctx.db.marriage_participant().insert(MarriageParticipant {
                    character_id,
                    marriage_id: marriage_id.clone(),
                });
            }
        }
        if let Some(mut courtship) = ctx.db.courtship().id().find(&courtship_id)
            && courtship.status != CourtshipStatus::Ended
        {
            courtship.status = CourtshipStatus::Ended;
            courtship.resolved_minute = Some(commitment.effective_minute);
            courtship.terminal_reason = Some(CourtshipTerminalReason::EngagementScheduled);
            ctx.db.courtship().id().update(courtship);
        }
        transition_commitment_terminal(
            ctx,
            commitment,
            CommitmentStatus::Fulfilled,
            CommitmentTerminalReason::WeddingCompleted,
            effective_minute,
        );
    }
    Ok(())
}

/// Settle a stable, bounded slice of due engagements without requiring either
/// participant's clock to be accessed. Active exclusivity guarantees that
/// delegating each selected row through its first participant cannot expand
/// the batch.
pub fn settle_due_weddings_global(
    ctx: &ReducerContext,
    now: u64,
    limit: usize,
) -> Result<usize, String> {
    let mut due: Vec<_> = ctx
        .db
        .exclusive_commitment()
        .iter()
        .filter(|row| {
            row.status == CommitmentStatus::Reserved
                && row.kind == CommitmentKind::Engagement
                && row.effective_minute <= now
        })
        .collect();
    due.sort_by(|left, right| {
        (left.effective_minute, left.id.as_str()).cmp(&(right.effective_minute, right.id.as_str()))
    });
    due.truncate(limit);
    let count = due.len();
    for commitment in due {
        settle_due_weddings(ctx, commitment.first_character_id, now)?;
    }
    Ok(count)
}

pub fn establish_pregnancy(
    ctx: &ReducerContext,
    mother_id: u64,
    father_id: u64,
    conceived_minute: u64,
    birth_settlement_id: &str,
) -> Result<Pregnancy, String> {
    if let Some(existing) = ctx
        .db
        .active_pregnancy()
        .mother_id()
        .find(mother_id)
        .and_then(|active| ctx.db.pregnancy().id().find(&active.pregnancy_id))
    {
        return Ok(existing);
    }
    let ordinal = ctx.db.pregnancy().mother_id().filter(mother_id).count() as u64;
    let due_minute = conceived_minute.saturating_add(GESTATION_MINUTES);
    if ctx
        .db
        .settlement()
        .id()
        .find(birth_settlement_id.to_owned())
        .is_none()
    {
        return Err("Pregnancy requires a valid conception settlement".into());
    }
    let birth_residence_character_id = [mother_id, father_id].into_iter().find_map(|parent_id| {
        ctx.db
            .residence_transition()
            .iter()
            .filter(|transition| {
                transition.affected_character_id == parent_id
                    && transition.minute <= conceived_minute
                    && matches!(
                        transition.kind,
                        ResidenceTransitionKind::OccupantAdmitted
                            | ResidenceTransitionKind::OccupantRemoved
                    )
            })
            .max_by_key(|transition| {
                (
                    transition.minute,
                    matches!(transition.kind, ResidenceTransitionKind::OccupantAdmitted),
                )
            })
            .filter(|transition| transition.kind == ResidenceTransitionKind::OccupantAdmitted)
            .map(|transition| transition.residence_character_id)
    });
    let seeds = deterministic_child_seeds(
        &mother_id.to_string(),
        &father_id.to_string(),
        ordinal,
        due_minute,
        birth_settlement_id,
    );
    let mut reserved_child_id = seeds.identity;
    while ctx.db.character().id().find(reserved_child_id).is_some()
        || ctx
            .db
            .pregnancy()
            .iter()
            .any(|row| row.reserved_child_id == reserved_child_id)
    {
        reserved_child_id = reserved_child_id.wrapping_add(1);
    }
    let id = format!("pregnancy:{mother_id}:{ordinal}");
    let pregnancy = Pregnancy {
        id: id.clone(),
        mother_id,
        father_id,
        ordinal,
        conceived_minute,
        due_minute,
        reserved_child_id,
        child_name_seed: seeds.name,
        child_female: seeds.female,
        child_home_seed: seeds.home,
        birth_settlement_id: birth_settlement_id.to_owned(),
        birth_residence_character_id,
        status: PregnancyStatus::Active,
        birth_character_id: None,
        resolved_minute: None,
    };
    ctx.db.pregnancy().insert(pregnancy.clone());
    ctx.db
        .child_identity_reservation()
        .insert(ChildIdentityReservation {
            character_id: reserved_child_id,
            pregnancy_id: id.clone(),
            reserved_minute: conceived_minute,
        });
    ctx.db.active_pregnancy().insert(ActivePregnancy {
        mother_id,
        pregnancy_id: id,
    });
    Ok(pregnancy)
}

fn conception_parents(
    ctx: &ReducerContext,
    first_id: u64,
    second_id: u64,
    trial_minute: u64,
) -> Result<Option<(u64, u64)>, String> {
    let first = ctx
        .db
        .character()
        .id()
        .find(first_id)
        .ok_or("First spouse not found")?;
    let second = ctx
        .db
        .character()
        .id()
        .find(second_id)
        .ok_or("Second spouse not found")?;
    let alive_at = |character_id: u64, alive_now: bool| {
        alive_now
            || ctx.db.strategic_corpse().iter().any(|corpse| {
                corpse.subject_character_id == Some(character_id)
                    && corpse.death_minute > trial_minute
            })
    };
    let adult_at = |character_id: u64, age_years: u16| {
        age_years >= ADULT_AGE_YEARS
            && ctx
                .db
                .pregnancy()
                .iter()
                .find(|pregnancy| pregnancy.birth_character_id == Some(character_id))
                .is_none_or(|birth| {
                    birth.due_minute.saturating_add(
                        u64::from(ADULT_AGE_YEARS)
                            * adventuresim_core::strategic_time::MINUTES_PER_YEAR,
                    ) <= trial_minute
                })
    };
    let married_at_trial = ctx.db.marriage().iter().any(|marriage| {
        ((marriage.first_character_id == first_id && marriage.second_character_id == second_id)
            || (marriage.first_character_id == second_id
                && marriage.second_character_id == first_id))
            && marriage.married_minute <= trial_minute
            && marriage
                .resolved_minute
                .is_none_or(|resolved| resolved > trial_minute)
    });
    if !alive_at(first_id, first.alive)
        || !alive_at(second_id, second.alive)
        || !adult_at(first_id, first.age_years)
        || !adult_at(second_id, second.age_years)
        || !married_at_trial
    {
        return Ok(None);
    }
    let first_personality = ctx
        .db
        .character_personality()
        .character_id()
        .find(first_id)
        .ok_or("First spouse personality not found")?;
    let second_personality = ctx
        .db
        .character_personality()
        .character_id()
        .find(second_id)
        .ok_or("Second spouse personality not found")?;
    let (mother_id, father_id) = match (first_personality.sex, second_personality.sex) {
        (Sex::Female, Sex::Male) => (first_id, second_id),
        (Sex::Male, Sex::Female) => (second_id, first_id),
        _ => return Ok(None),
    };
    Ok(Some((mother_id, father_id)))
}

fn refresh_spouse_pair_morale(
    ctx: &ReducerContext,
    first_id: u64,
    second_id: u64,
    joint_minutes: u64,
    minute: u64,
) -> Result<(), String> {
    let earned = spouse_leisure_earned_milli(joint_minutes);
    if earned == 0 {
        return Ok(());
    }
    let source = format!(
        "spouse-leisure:{}:{}",
        first_id.min(second_id),
        first_id.max(second_id)
    );
    for character_id in [first_id, second_id] {
        let existing = ctx
            .db
            .morale_event()
            .character_id()
            .filter(character_id)
            .find(|event| event.source_id.as_deref() == Some(&source));
        let residence_source = format!("residence-leisure:{character_id}");
        let residence = ctx
            .db
            .morale_event()
            .character_id()
            .filter(character_id)
            .find(|event| event.source_id.as_deref() == Some(&residence_source))
            .map_or(Default::default(), |event| {
                adventuresim_core::courtship::RefreshableMorale {
                    milli_points: (event.magnitude.max(0.0) * 1_000.0).round() as u32,
                    expires_at_minute: event.expires_at_minute,
                }
            });
        let refreshed = refresh_bounded_leisure_morale(
            existing.as_ref().map_or(Default::default(), |event| {
                adventuresim_core::courtship::RefreshableMorale {
                    milli_points: (event.magnitude.max(0.0) * 1_000.0).round() as u32,
                    expires_at_minute: event.expires_at_minute,
                }
            }),
            residence,
            minute,
            earned,
            SPOUSE_LEISURE_MORALE_SPEC,
        );
        crate::condition::upsert_fixed_morale_event_without_refresh(
            ctx,
            character_id,
            "spouse_leisure",
            refreshed.milli_points as f32 / 1_000.0,
            minute,
            refreshed.expires_at_minute,
            &source,
        );
        crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    }
    Ok(())
}

fn settle_spouse_leisure_pair(
    ctx: &ReducerContext,
    first_id: u64,
    second_id: u64,
) -> Result<(), String> {
    let (first_id, second_id) = canonical_pair(first_id, second_id);
    let pair_id = format!("spouse-leisure:{first_id}:{second_id}");
    let mut overlaps = Vec::new();
    for first in ctx
        .db
        .spouse_leisure_slice()
        .character_id()
        .filter(first_id)
    {
        for second in ctx
            .db
            .spouse_leisure_slice()
            .character_id()
            .filter(second_id)
        {
            let id = format!("spouse-overlap:{}:{}", first.id, second.id);
            if ctx.db.spouse_leisure_overlap().id().find(&id).is_some() {
                continue;
            }
            let joint = joint_leisure_minutes(
                LeisureInterval {
                    start_minute: first.start_minute,
                    end_minute: first.end_minute,
                    location_id: &first.location_id,
                },
                LeisureInterval {
                    start_minute: second.start_minute,
                    end_minute: second.end_minute,
                    location_id: &second.location_id,
                },
            );
            if joint > 0 {
                let start = first.start_minute.max(second.start_minute);
                let end = first.end_minute.min(second.end_minute);
                overlaps.push((
                    start,
                    end,
                    first.location_id.clone(),
                    id,
                    first.id.clone(),
                    second.id.clone(),
                    joint,
                ));
            }
        }
    }
    overlaps.sort_by(|left, right| (left.0, &left.1).cmp(&(right.0, &right.1)));
    for (overlap_start, overlap_end, location_id, id, first_slice_id, second_slice_id, joint) in
        overlaps
    {
        let mut accrual = ctx
            .db
            .spouse_leisure_accrual()
            .pair_id()
            .find(&pair_id)
            .unwrap_or(SpouseLeisureAccrual {
                pair_id: pair_id.clone(),
                first_character_id: first_id,
                second_character_id: second_id,
                conserved_joint_minutes: 0,
                next_trial_ordinal: 0,
                total_joint_minutes: 0,
            });
        let plan = conception_quantum_plan(
            ConceptionQuantumState {
                conserved_joint_minutes: accrual.conserved_joint_minutes,
                next_trial_ordinal: accrual.next_trial_ordinal,
            },
            joint,
        );
        for trial in &plan.trials {
            let receipt_id = format!("conception-trial:{pair_id}:{}", trial.ordinal);
            if ctx
                .db
                .conception_trial_receipt()
                .id()
                .find(&receipt_id)
                .is_some()
            {
                continue;
            }
            let minute = overlap_start.saturating_add(trial.crossing_offset_minutes);
            let ordinal = trial.ordinal.to_string();
            let entropy = (stable_lifecycle_hash(
                "spouse-conception",
                &[&first_id.to_string(), &second_id.to_string(), &ordinal],
            ) % 10_000) as u16;
            let parents = conception_parents(ctx, first_id, second_id, minute)?;
            let succeeded = parents.is_some()
                && succeeds_daily_trial(entropy, CONCEPTION_CHANCE_PER_TEN_THOUSAND)
                && parents.is_some_and(|(mother_id, _)| {
                    !ctx.db
                        .pregnancy()
                        .mother_id()
                        .filter(mother_id)
                        .any(|pregnancy| {
                            pregnancy.conceived_minute <= minute && minute < pregnancy.due_minute
                        })
                });
            ctx.db
                .conception_trial_receipt()
                .insert(ConceptionTrialReceipt {
                    id: receipt_id,
                    pair_id: pair_id.clone(),
                    ordinal: trial.ordinal,
                    minute,
                    succeeded,
                });
            if succeeded && let Some((mother_id, father_id)) = parents {
                establish_pregnancy(ctx, mother_id, father_id, minute, &location_id)?;
            }
        }
        accrual.conserved_joint_minutes = plan.state.conserved_joint_minutes;
        accrual.next_trial_ordinal = plan.state.next_trial_ordinal;
        accrual.total_joint_minutes = accrual.total_joint_minutes.saturating_add(joint);
        if ctx
            .db
            .spouse_leisure_accrual()
            .pair_id()
            .find(&pair_id)
            .is_some()
        {
            ctx.db.spouse_leisure_accrual().pair_id().update(accrual);
        } else {
            ctx.db.spouse_leisure_accrual().insert(accrual);
        }
        let resolved_minute = overlap_end;
        ctx.db
            .spouse_leisure_overlap()
            .insert(SpouseLeisureOverlap {
                id,
                first_slice_id,
                second_slice_id,
                joint_minutes: joint,
                resolved_minute,
            });
        refresh_spouse_pair_morale(ctx, first_id, second_id, joint, resolved_minute)?;
    }
    Ok(())
}

pub fn apply_spouse_leisure_conception(
    ctx: &ReducerContext,
    character_id: u64,
    interval_start: u64,
    interval_end: u64,
    schedule: DailySchedule,
) -> Result<(), String> {
    if interval_end <= interval_start {
        return Ok(());
    }
    let Some(marriage) = ctx.db.marriage().iter().find(|row| {
        (row.first_character_id == character_id || row.second_character_id == character_id)
            && row.married_minute < interval_end
            && row
                .resolved_minute
                .is_none_or(|resolved| resolved > interval_start)
    }) else {
        return Ok(());
    };
    let spouse_id = if marriage.first_character_id == character_id {
        marriage.second_character_id
    } else {
        marriage.first_character_id
    };
    let interval_start = interval_start.max(marriage.married_minute);
    let interval_end = interval_end.min(marriage.resolved_minute.unwrap_or(u64::MAX));
    if interval_end <= interval_start {
        return Ok(());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let Some(location_id) = character.current_settlement_id else {
        return Ok(());
    };
    for realized in restorative_leisure_spans(
        schedule,
        interval_start,
        interval_end.saturating_sub(interval_start),
    ) {
        let existing: Vec<_> = ctx
            .db
            .spouse_leisure_slice()
            .character_id()
            .filter(character_id)
            .filter(|slice| slice.location_id == location_id)
            .map(|slice| MinuteSpan {
                start_minute: slice.start_minute,
                end_minute: slice.end_minute,
            })
            .collect();
        for uncovered in uncovered_minute_spans(
            MinuteSpan {
                start_minute: realized.start_minute,
                end_minute: realized.end_minute,
            },
            existing,
        ) {
            let id = format!(
                "spouse-leisure-slice:{character_id}:{}:{}:{location_id}",
                uncovered.start_minute, uncovered.end_minute
            );
            ctx.db.spouse_leisure_slice().insert(SpouseLeisureSlice {
                id,
                character_id,
                start_minute: uncovered.start_minute,
                end_minute: uncovered.end_minute,
                location_id: location_id.clone(),
            });
        }
    }
    settle_spouse_leisure_pair(ctx, character_id, spouse_id)
}

/// Colocated spouses refresh a durable morale benefit from qualifying Leisure.
/// The source is pair-stable, so repeated leisure refreshes rather than stacks
/// unbounded events and remains independent of a residence comfort bonus.
pub fn apply_spouse_leisure_morale(
    _ctx: &ReducerContext,
    _character_id: u64,
    _interval_end: u64,
    _qualifying_leisure_minutes: u64,
) -> Result<(), String> {
    // Compatibility seam: conception registration now settles the conserved
    // overlap and refreshes the bounded benefit for both spouses exactly once.
    Ok(())
}

/// Materialize due children as ordinary full Characters under NPC policy.
/// Age-restricted behavior remains elsewhere, but the child already has the
/// complete data/skills/needs surface and canonical family edges.
pub fn settle_due_births(ctx: &ReducerContext, mother_id: u64, now: u64) -> Result<(), String> {
    let due: Vec<_> = ctx
        .db
        .active_pregnancy()
        .mother_id()
        .find(mother_id)
        .and_then(|active| ctx.db.pregnancy().id().find(&active.pregnancy_id))
        .filter(|pregnancy| {
            pregnancy.status == PregnancyStatus::Active && pregnancy.due_minute <= now
        })
        .into_iter()
        .collect();
    for mut pregnancy in due {
        let mother = ctx
            .db
            .character()
            .id()
            .find(pregnancy.mother_id)
            .ok_or("Pregnant mother not found")?;
        let father = ctx
            .db
            .character()
            .id()
            .find(pregnancy.father_id)
            .ok_or("Pregnant father not found")?;
        let child_id = pregnancy.reserved_child_id;
        if ctx.db.character().id().find(child_id).is_some() {
            return Err("Reserved child identity is no longer available".into());
        }
        let settlement_id = pregnancy.birth_settlement_id.clone();
        let newborn_life = crate::character::NpcLifeFacts {
            age_years: 0,
            organization_id: None,
            literacy: None,
        };
        crate::character::insert_character_with_origin(
            ctx,
            format!("Child-{:08x}", pregnancy.child_name_seed as u32),
            child_id,
            crate::character::CharacterCreationOptions {
                origin_settlement_id: Some(&settlement_id),
                mode: crate::character::CharacterCreationMode::Newborn,
                create_solo_party: false,
                stable_seed: pregnancy.child_name_seed,
                initial_time_minute: Some(pregnancy.due_minute),
            },
            None,
            Some(&newborn_life),
        )?;
        record_character_birth(
            ctx,
            child_id,
            i64::try_from(pregnancy.due_minute).unwrap_or(i64::MAX),
        );
        if let Some(mut personality) = ctx.db.character_personality().character_id().find(child_id)
        {
            personality.sex = if pregnancy.child_female {
                Sex::Female
            } else {
                Sex::Male
            };
            ctx.db
                .character_personality()
                .character_id()
                .update(personality);
        }
        initialize_npc_policy(
            ctx,
            child_id,
            settlement_id.clone(),
            pregnancy.child_home_seed,
        )?;
        ctx.db
            .child_identity_reservation()
            .character_id()
            .delete(child_id);
        ensure_kinship(
            ctx,
            child_id,
            mother.id,
            KinshipKind::Parent,
            pregnancy.due_minute,
        );
        ensure_kinship(
            ctx,
            child_id,
            father.id,
            KinshipKind::Parent,
            pregnancy.due_minute,
        );
        ensure_kinship(
            ctx,
            mother.id,
            child_id,
            KinshipKind::Child,
            pregnancy.due_minute,
        );
        ensure_kinship(
            ctx,
            father.id,
            child_id,
            KinshipKind::Child,
            pregnancy.due_minute,
        );
        if let Some(household_member) = ctx
            .db
            .household_member()
            .character_id()
            .find(mother.id)
            .or_else(|| ctx.db.household_member().character_id().find(father.id))
        {
            join_household(
                ctx,
                &household_member.household_id,
                child_id,
                pregnancy.due_minute,
                HouseholdRole::Dependent,
            );
        }
        if let Some(residence_character_id) =
            pregnancy.birth_residence_character_id.filter(|holder_id| {
                ctx.db
                    .character_residence()
                    .character_id()
                    .find(*holder_id)
                    .is_some_and(|residence| {
                        residence.active && residence.settlement_id == settlement_id
                    })
            })
        {
            crate::residence::move_residence_occupant_internal(
                ctx,
                residence_character_id,
                child_id,
                pregnancy.due_minute,
            )
            .expect("birth residence was prevalidated");
        }
        pregnancy.status = PregnancyStatus::Born;
        pregnancy.birth_character_id = Some(child_id);
        pregnancy.resolved_minute = Some(pregnancy.due_minute);
        ctx.db.pregnancy().id().update(pregnancy.clone());
        if ctx
            .db
            .active_pregnancy()
            .mother_id()
            .find(pregnancy.mother_id)
            .is_some_and(|active| active.pregnancy_id == pregnancy.id)
        {
            ctx.db
                .active_pregnancy()
                .mother_id()
                .delete(pregnancy.mother_id);
        }
    }
    Ok(())
}

fn validate_due_birth(ctx: &ReducerContext, pregnancy: &Pregnancy) -> Result<(), String> {
    if pregnancy.status != PregnancyStatus::Active {
        return Err("Pregnancy is not active".into());
    }
    if ctx.db.character().id().find(pregnancy.mother_id).is_none()
        || ctx.db.character().id().find(pregnancy.father_id).is_none()
    {
        return Err("Birth parents are unavailable".into());
    }
    if ctx
        .db
        .settlement()
        .id()
        .find(&pregnancy.birth_settlement_id)
        .is_none()
    {
        return Err("Birth settlement is unavailable".into());
    }
    if ctx
        .db
        .child_identity_reservation()
        .character_id()
        .find(pregnancy.reserved_child_id)
        .is_none_or(|row| row.pregnancy_id != pregnancy.id)
        || ctx
            .db
            .character()
            .id()
            .find(pregnancy.reserved_child_id)
            .is_some()
    {
        return Err("Reserved child identity is unavailable".into());
    }
    if let Some(holder_id) = pregnancy.birth_residence_character_id
        && ctx
            .db
            .character_residence()
            .character_id()
            .find(holder_id)
            .is_none_or(|residence| {
                !residence.active || residence.settlement_id != pregnancy.birth_settlement_id
            })
    {
        return Err("Birth residence is unavailable".into());
    }
    Ok(())
}

/// Materialize a stable, bounded slice of due pregnancies independently of
/// parent access. Each mother can have only one active pregnancy, so the
/// per-mother settlement call cannot exceed this selected batch.
pub fn settle_due_births_global(
    ctx: &ReducerContext,
    now: u64,
    limit: usize,
) -> Result<usize, String> {
    let mut due: Vec<_> = ctx
        .db
        .pregnancy()
        .iter()
        .filter(|row| row.status == PregnancyStatus::Active && row.due_minute <= now)
        .collect();
    due.sort_by(|left, right| {
        (left.due_minute, left.id.as_str()).cmp(&(right.due_minute, right.id.as_str()))
    });
    due.truncate(limit);
    let count = due.len();
    for pregnancy in due {
        settle_due_births(ctx, pregnancy.mother_id, now)?;
    }
    Ok(count)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DueLifecycleEvent {
    Wedding {
        effective_minute: u64,
        id: String,
        participant_id: u64,
    },
    Birth {
        effective_minute: u64,
        id: String,
        mother_id: u64,
    },
}

impl DueLifecycleEvent {
    /// Weddings precede births at the same minute. This precedence is part of
    /// the persistence contract, not an accident of table traversal order.
    fn stable_key(&self) -> (u64, u8, &str) {
        match self {
            Self::Wedding {
                effective_minute,
                id,
                ..
            } => (*effective_minute, 0, id),
            Self::Birth {
                effective_minute,
                id,
                ..
            } => (*effective_minute, 1, id),
        }
    }
}

fn record_lifecycle_failure(
    ctx: &ReducerContext,
    event_kind: LifecycleEventKind,
    event_id: &str,
    effective_minute: u64,
    recorded_minute: u64,
    error: String,
) {
    let id = format!("lifecycle-failure:{event_kind:?}:{event_id}:{effective_minute}");
    if ctx.db.lifecycle_event_failure().id().find(&id).is_none() {
        ctx.db
            .lifecycle_event_failure()
            .insert(LifecycleEventFailure {
                id,
                event_kind,
                event_id: event_id.to_owned(),
                effective_minute,
                recorded_minute,
                error: error.chars().take(512).collect(),
            });
    }
}

fn quarantine_invalid_birth(ctx: &ReducerContext, pregnancy_id: &str, effective_minute: u64) {
    let Some(mut pregnancy) = ctx.db.pregnancy().id().find(&pregnancy_id.to_owned()) else {
        return;
    };
    if pregnancy.status != PregnancyStatus::Active {
        return;
    }
    pregnancy.status = PregnancyStatus::Ended;
    pregnancy.resolved_minute = Some(effective_minute);
    ctx.db.pregnancy().id().update(pregnancy.clone());
    if ctx
        .db
        .active_pregnancy()
        .mother_id()
        .find(pregnancy.mother_id)
        .is_some_and(|active| active.pregnancy_id == pregnancy.id)
    {
        ctx.db
            .active_pregnancy()
            .mother_id()
            .delete(pregnancy.mother_id);
    }
    if ctx
        .db
        .child_identity_reservation()
        .character_id()
        .find(pregnancy.reserved_child_id)
        .is_some_and(|reservation| reservation.pregnancy_id == pregnancy.id)
    {
        ctx.db
            .child_identity_reservation()
            .character_id()
            .delete(pregnancy.reserved_child_id);
    }
}

/// Process one hard-bounded chronological queue across every globally due
/// relationship event. Indexed range scans cap each source before the merge;
/// this never collects either full table. Malformed gameplay rows are
/// terminalized and audited so a later valid row can proceed in the same
/// recurring invocation.
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
        .take(limit)
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
                .take(limit)
                .map(|row| DueLifecycleEvent::Birth {
                    effective_minute: row.due_minute,
                    id: row.id,
                    mother_id: row.mother_id,
                }),
        )
        .collect();
    due.sort_by(|left, right| left.stable_key().cmp(&right.stable_key()));
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
                        );
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

fn socializing_id(actor_id: u64, day: u64) -> String {
    format!("socializing:{actor_id}:{day}")
}

fn active_romantic_partner(ctx: &ReducerContext, actor_id: u64) -> Option<u64> {
    let actor_minute = canonical_now(ctx, actor_id).ok()?;
    let courtship = ctx.db.courtship().iter().find(|row| {
        row.status != CourtshipStatus::Ended
            && row.started_minute <= actor_minute
            && row
                .resolved_minute
                .is_none_or(|resolved| resolved > actor_minute)
            && (row.first_character_id == actor_id || row.second_character_id == actor_id)
    })?;
    Some(if courtship.first_character_id == actor_id {
        courtship.second_character_id
    } else {
        courtship.first_character_id
    })
}

fn socializing_target(ctx: &ReducerContext, actor_id: u64, day: u64) -> Option<u64> {
    let actor = ctx.db.character().id().find(actor_id)?;
    let same_settlement = |candidate: &crate::Character| {
        candidate.alive
            && candidate.id != actor_id
            && candidate.current_settlement_id == actor.current_settlement_id
    };
    let location_id = actor.current_settlement_id.as_deref()?;
    let choose = |mut candidates: Vec<u64>| {
        candidates.sort_unstable();
        candidates.dedup();
        let candidate_strings: Vec<_> = candidates.iter().map(u64::to_string).collect();
        let actor = actor_id.to_string();
        select_daily_location_target(
            &actor,
            location_id,
            day,
            candidate_strings.iter().map(String::as_str),
        )
        .and_then(|selected| selected.parse().ok())
    };
    if let Some(partner) = active_romantic_partner(ctx, actor_id)
        && ctx
            .db
            .character()
            .id()
            .find(partner)
            .is_some_and(|candidate| same_settlement(&candidate))
    {
        return Some(partner);
    }
    let party: Vec<_> = ctx
        .db
        .character()
        .iter()
        .filter(|candidate| {
            same_settlement(candidate)
                && candidate.party_id.is_some()
                && candidate.party_id == actor.party_id
        })
        .map(|candidate| candidate.id)
        .collect();
    if let Some(target) = choose(party) {
        return Some(target);
    }
    let acquainted: Vec<_> = ctx
        .db
        .character()
        .iter()
        .filter(|candidate| {
            same_settlement(candidate)
                && crate::social::current_affinity(ctx, candidate.id, actor_id) > 0.0
        })
        .map(|candidate| candidate.id)
        .collect();
    if let Some(target) = choose(acquainted) {
        return Some(target);
    }
    choose(
        ctx.db
            .character()
            .iter()
            .filter(|candidate| same_settlement(candidate))
            .map(|candidate| candidate.id)
            .collect(),
    )
}

/// Resolve scheduled Socializing without consuming another person's canonical
/// time.  The social edge is intentionally soft: existing engagements merely
/// change romantic eligibility, never prevent close friendship.
pub fn apply_scheduled_socializing(
    ctx: &ReducerContext,
    actor_id: u64,
    schedule_minutes_per_day: u16,
    interval_start: u64,
    interval_end: u64,
) -> Result<(), String> {
    if schedule_minutes_per_day == 0 || interval_end <= interval_start {
        return Ok(());
    }
    let first_day = interval_start / MINUTES_PER_DAY;
    let last_day = interval_end.saturating_sub(1) / MINUTES_PER_DAY;
    for day in first_day..=last_day {
        let day_start = day.saturating_mul(MINUTES_PER_DAY);
        let start = interval_start.max(day_start);
        let end = interval_end.min(day_start.saturating_add(MINUTES_PER_DAY));
        let allocation = |minute: u64| {
            minute
                .saturating_sub(day_start)
                .saturating_mul(u64::from(schedule_minutes_per_day))
                / MINUTES_PER_DAY
        };
        let id = socializing_id(actor_id, day);
        let existing = ctx.db.socializing_receipt().id().find(&id);
        let applied_through = existing
            .as_ref()
            .map_or(start, |receipt| receipt.end_minute.max(start));
        let minutes = allocation(end).saturating_sub(allocation(applied_through));
        if minutes == 0 {
            continue;
        }
        let target_id = if let Some(receipt) = existing.as_ref() {
            receipt.target_id
        } else {
            let Some(target_id) = socializing_target(ctx, actor_id, day) else {
                continue;
            };
            target_id
        };
        let _ =
            enforce_temporal_scope(ctx, actor_id, Some(target_id), TemporalScope::PairwiseSoft)?;
        let actor_party_id = ctx
            .db
            .character()
            .id()
            .find(actor_id)
            .and_then(|character| character.party_id);
        let target_is_party_member = actor_party_id.is_some()
            && ctx
                .db
                .character()
                .id()
                .find(target_id)
                .is_some_and(|character| character.party_id == actor_party_id);
        if target_is_party_member {
            crate::social::apply_async_socializing_without_familiarity(
                ctx, actor_id, target_id, minutes,
            )?;
        } else {
            crate::social::apply_async_socializing(ctx, actor_id, target_id, minutes)?;
        }
        let receipt = SocializingReceipt {
            id,
            actor_id,
            target_id,
            start_minute: existing
                .as_ref()
                .map_or(start, |receipt| receipt.start_minute.min(start)),
            end_minute: end,
            minutes: existing
                .as_ref()
                .map_or(minutes, |receipt| receipt.minutes.saturating_add(minutes)),
        };
        if existing.is_some() {
            ctx.db.socializing_receipt().id().update(receipt);
        } else {
            ctx.db.socializing_receipt().insert(receipt);
        }
    }
    Ok(())
}

/// Resolve the public-risk side of an informal relationship once per observer
/// and day.  The receipt makes it independent of time-advance chunking; only
/// living adult parents and siblings co-located with either partner observe.
pub fn settle_secret_courtship_discovery_for_pair(
    ctx: &ReducerContext,
    first_id: u64,
    second_id: u64,
    day: u64,
) -> Result<(), String> {
    let (first, second) = canonical_pair(first_id, second_id);
    let courtship_id = format!("courtship:{first}:{second}");
    let Some(courtship) = ctx.db.courtship().id().find(&courtship_id) else {
        return Ok(());
    };
    if courtship.kind != CourtshipKind::Informal || courtship.status != CourtshipStatus::Active {
        return Ok(());
    }
    let first_person = ctx
        .db
        .character()
        .id()
        .find(first)
        .ok_or("Courtship participant not found")?;
    let second_person = ctx
        .db
        .character()
        .id()
        .find(second)
        .ok_or("Courtship participant not found")?;
    let mut observers: Vec<_> = ctx
        .db
        .character_kinship()
        .iter()
        .filter(|edge| {
            (edge.subject_id == first || edge.subject_id == second)
                && matches!(edge.kind, KinshipKind::Parent | KinshipKind::Sibling)
        })
        .map(|edge| edge.related_id)
        .collect();
    observers.sort_unstable();
    observers.dedup();
    for observer_id in observers {
        let id = format!("discovery:{courtship_id}:{observer_id}:{day}");
        if ctx.db.courtship_discovery().id().find(&id).is_some() {
            continue;
        }
        let Some(observer) = ctx.db.character().id().find(observer_id) else {
            continue;
        };
        if !observer.alive
            || effective_age_years(ctx, observer_id, day.saturating_mul(MINUTES_PER_DAY))
                .unwrap_or(observer.age_years)
                < ADULT_AGE_YEARS
            || first_person.current_settlement_id != second_person.current_settlement_id
            || observer.current_settlement_id != first_person.current_settlement_id
        {
            continue;
        }
        let insight = ctx
            .db
            .character_skills()
            .character_id()
            .find(observer_id)
            .map_or(0.0, |skills| skills.insight_hours.sqrt());
        let deception = [first, second]
            .into_iter()
            .filter_map(|id| ctx.db.character_skills().character_id().find(id))
            .map(|skills| skills.deception_hours.sqrt())
            .fold(f32::INFINITY, f32::min);
        let deception = if deception.is_finite() {
            deception
        } else {
            0.0
        };
        let entropy =
            ((first ^ second ^ observer_id ^ day.rotate_left(19)) % 10_000) as f32 / 10_000.0;
        let discovery_chance = ((insight - deception) * 0.08 + 0.15).clamp(0.02, 0.85);
        let succeeded = entropy < discovery_chance;
        let attempted_minute = day
            .saturating_mul(MINUTES_PER_DAY)
            .max(courtship.started_minute);
        ctx.db.courtship_discovery().insert(CourtshipDiscovery {
            id,
            courtship_id: courtship_id.clone(),
            observer_id,
            day,
            attempted_minute,
            succeeded,
            observer_insight: insight,
            weaker_deception: deception,
        });
        if succeeded {
            if let Some(mut active) = ctx.db.courtship().id().find(&courtship_id) {
                active.status = CourtshipStatus::Exposed;
                ctx.db.courtship().id().update(active);
            }
            let anchor_minute = ctx
                .db
                .character_time()
                .character_id()
                .find(observer_id)
                .map_or(attempted_minute, |time| time.minutes);
            for participant_id in [first, second] {
                let affinity_id = format!("{observer_id}:{participant_id}");
                let row = CharacterAffinity {
                    id: affinity_id.clone(),
                    subject_id: observer_id,
                    actor_id: participant_id,
                    anchor: (crate::social::current_affinity(ctx, observer_id, participant_id)
                        - 8.0)
                        .clamp(-100.0, 100.0),
                    anchor_minute,
                };
                if ctx
                    .db
                    .character_affinity()
                    .id()
                    .find(&affinity_id)
                    .is_some()
                {
                    ctx.db.character_affinity().id().update(row);
                } else {
                    ctx.db.character_affinity().insert(row);
                }
            }
            break;
        }
    }
    Ok(())
}

/// Advance all active secret relationships involving this character through
/// the current relationship day. This is independent of whom the character
/// happened to socialize with: every eligible family observer gets one
/// receipt per day until the first successful exposure.
pub fn settle_secret_courtship_discovery_for_character(
    ctx: &ReducerContext,
    character_id: u64,
    minute: u64,
) -> Result<(), String> {
    let current_day = minute / MINUTES_PER_DAY;
    let mut courtship_ids: Vec<_> = ctx
        .db
        .courtship()
        .iter()
        .filter(|row| {
            row.kind == CourtshipKind::Informal
                && row.status == CourtshipStatus::Active
                && (row.first_character_id == character_id
                    || row.second_character_id == character_id)
                && row.started_minute <= minute
        })
        .map(|row| row.id)
        .collect();
    courtship_ids.sort();
    for courtship_id in courtship_ids {
        loop {
            let Some(courtship) = ctx.db.courtship().id().find(&courtship_id) else {
                break;
            };
            if courtship.status != CourtshipStatus::Active
                || courtship.next_discovery_day > current_day
            {
                break;
            }
            let day = courtship.next_discovery_day;
            settle_secret_courtship_discovery_for_pair(
                ctx,
                courtship.first_character_id,
                courtship.second_character_id,
                day,
            )?;
            let Some(mut updated) = ctx.db.courtship().id().find(&courtship_id) else {
                break;
            };
            if updated.status != CourtshipStatus::Active {
                break;
            }
            updated.next_discovery_day = day.saturating_add(1);
            ctx.db.courtship().id().update(updated);
        }
    }
    Ok(())
}

fn personality_disposition(value: PersonalityCourtship) -> CourtshipDisposition {
    match value {
        PersonalityCourtship::Amorous => CourtshipDisposition::Amorous,
        PersonalityCourtship::Neutral => CourtshipDisposition::Neutral,
        PersonalityCourtship::Proper => CourtshipDisposition::Proper,
    }
}

fn inclination_accepts(inclination: Inclination, presentation: Presentation) -> bool {
    matches!(inclination, Inclination::Either)
        || matches!(
            (inclination, presentation),
            (Inclination::Men, Presentation::Man) | (Inclination::Women, Presentation::Woman)
        )
}

fn validate_canonical_courtship_pair(
    ctx: &ReducerContext,
    suitor_id: u64,
    partner_id: u64,
) -> Result<u64, String> {
    if suitor_id == partner_id {
        return Err("A character cannot court themself".into());
    }
    let suitor = ctx
        .db
        .character()
        .id()
        .find(suitor_id)
        .ok_or("Suitor not found")?;
    let partner = ctx
        .db
        .character()
        .id()
        .find(partner_id)
        .ok_or("Potential partner not found")?;
    let effective_minute = enforce_temporal_scope(
        ctx,
        suitor_id,
        Some(partner_id),
        TemporalScope::ExclusiveShared,
    )?;
    if !suitor.alive
        || !partner.alive
        || effective_age_years(ctx, suitor_id, effective_minute).unwrap_or(suitor.age_years)
            < ADULT_AGE_YEARS
        || effective_age_years(ctx, partner_id, effective_minute).unwrap_or(partner.age_years)
            < ADULT_AGE_YEARS
    {
        return Err("Courtship requires two living adult characters".into());
    }
    if suitor.current_settlement_id != partner.current_settlement_id {
        return Err("Courtship requires co-location".into());
    }
    let suitor_personality = ctx
        .db
        .character_personality()
        .character_id()
        .find(suitor_id)
        .ok_or("Suitor personality not found")?;
    let partner_personality = ctx
        .db
        .character_personality()
        .character_id()
        .find(partner_id)
        .ok_or("Partner personality not found")?;
    if !inclination_accepts(
        suitor_personality.inclination,
        partner_personality.presentation,
    ) || !inclination_accepts(
        partner_personality.inclination,
        suitor_personality.presentation,
    ) {
        return Err("This pair does not have mutual attraction".into());
    }
    if ctx
        .db
        .exclusive_commitment_participant()
        .character_id()
        .find(suitor_id)
        .is_some()
        || ctx
            .db
            .exclusive_commitment_participant()
            .character_id()
            .find(partner_id)
            .is_some()
        || ctx
            .db
            .marriage_participant()
            .character_id()
            .find(suitor_id)
            .is_some()
        || ctx
            .db
            .marriage_participant()
            .character_id()
            .find(partner_id)
            .is_some()
    {
        return Err("An exclusive romantic commitment prevents new courtship".into());
    }
    if ctx
        .db
        .character_kinship()
        .iter()
        .any(|edge| edge.subject_id == suitor_id && edge.related_id == partner_id)
    {
        return Err("Close relatives cannot court".into());
    }
    Ok(effective_minute)
}

fn establish_courtship(
    ctx: &ReducerContext,
    suitor_id: u64,
    partner_id: u64,
    kind: CourtshipKind,
    secrecy_reason: Option<CourtshipSecrecyReason>,
    minute: u64,
) -> Result<(), String> {
    let (first_character_id, second_character_id) = canonical_pair(suitor_id, partner_id);
    let id = format!("courtship:{first_character_id}:{second_character_id}");
    if let Some(existing) = ctx.db.courtship().id().find(&id) {
        return match (existing.status, existing.kind == kind) {
            (CourtshipStatus::Active | CourtshipStatus::Exposed, true) => Ok(()),
            (CourtshipStatus::Active | CourtshipStatus::Exposed, false) => {
                Err("This pair already has an active courtship of another kind".into())
            }
            (CourtshipStatus::Ended, _) => {
                Err("Ended courtship history is final for this pair".into())
            }
        };
    }
    ctx.db.courtship().insert(CourtshipRecord {
        id,
        first_character_id,
        second_character_id,
        kind,
        status: CourtshipStatus::Active,
        secrecy_reason,
        started_minute: minute,
        next_discovery_day: minute / MINUTES_PER_DAY,
        resolved_minute: None,
        terminal_reason: None,
    });
    Ok(())
}

#[reducer]
pub fn begin_formal_courtship(
    ctx: &ReducerContext,
    suitor_id: u64,
    partner_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, suitor_id)?;
    let minute = validate_canonical_courtship_pair(ctx, suitor_id, partner_id)?;
    let suitor = ctx
        .db
        .character_personality()
        .character_id()
        .find(suitor_id)
        .ok_or("Suitor personality not found")?;
    let partner = ctx
        .db
        .character_personality()
        .character_id()
        .find(partner_id)
        .ok_or("Partner personality not found")?;
    if suitor.sex != Sex::Male || partner.sex != Sex::Female {
        return Err("Formal courtship currently requires a man suitor and woman partner".into());
    }
    if crate::social::current_affinity(ctx, partner_id, suitor_id) < FORMAL_COURTSHIP_AFFINITY {
        return Err("The prospective partner does not yet have enough affinity".into());
    }
    let father =
        father_of(ctx, partner_id).ok_or("Formal courtship requires a known living father")?;
    if crate::social::current_affinity(ctx, father, suitor_id) < FORMAL_FATHER_APPROVAL_AFFINITY {
        return Err("Her father does not approve of this suitor".into());
    }
    establish_courtship(
        ctx,
        suitor_id,
        partner_id,
        CourtshipKind::Formal,
        None,
        minute,
    )
}

#[reducer]
pub fn begin_informal_courtship(
    ctx: &ReducerContext,
    suitor_id: u64,
    partner_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, suitor_id)?;
    let minute = validate_canonical_courtship_pair(ctx, suitor_id, partner_id)?;
    let partner = ctx
        .db
        .character_personality()
        .character_id()
        .find(partner_id)
        .ok_or("Partner personality not found")?;
    if crate::social::current_affinity(ctx, partner_id, suitor_id)
        < informal_affinity_threshold(personality_disposition(partner.courtship))
    {
        return Err(
            "The prospective partner does not yet have enough affinity for informal courtship"
                .into(),
        );
    }
    let suitor_personality = ctx
        .db
        .character_personality()
        .character_id()
        .find(suitor_id)
        .ok_or("Suitor personality not found")?;
    let formal_pair = suitor_personality.sex == Sex::Male && partner.sex == Sex::Female;
    let living_father = father_of(ctx, partner_id);
    let father_approves = living_father.is_some_and(|father| {
        crate::social::current_affinity(ctx, father, suitor_id) >= FORMAL_FATHER_APPROVAL_AFFINITY
    });
    if formal_pair && father_approves {
        return Err("Her father's approval makes the formal route available".into());
    }
    let secrecy_reason = if formal_pair && living_father.is_some() {
        CourtshipSecrecyReason::FatherDisapproval
    } else {
        CourtshipSecrecyReason::FormalRouteUnavailable
    };
    establish_courtship(
        ctx,
        suitor_id,
        partner_id,
        CourtshipKind::Informal,
        Some(secrecy_reason),
        minute,
    )
}

/// A year-long engagement is the first exclusive relationship claim.  It is
/// deliberately later than courtship, so two people may still have ordinary
/// soft social relationships until either pair chooses the public commitment.
#[reducer]
pub fn schedule_wedding(
    ctx: &ReducerContext,
    first_character_id: u64,
    second_character_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, first_character_id)?;
    let minute = validate_canonical_courtship_pair(ctx, first_character_id, second_character_id)?;
    let (first, second) = canonical_pair(first_character_id, second_character_id);
    let courtship_id = format!("courtship:{first}:{second}");
    if !ctx
        .db
        .courtship()
        .id()
        .find(&courtship_id)
        .is_some_and(|courtship| courtship.status != CourtshipStatus::Ended)
    {
        return Err("A wedding requires an active courtship".into());
    }
    reserve_wedding(ctx, first, second, minute).map(|_| ())
}

#[reducer]
pub fn cancel_wedding(
    ctx: &ReducerContext,
    actor_id: u64,
    commitment_id: String,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, actor_id)?;
    let commitment = ctx
        .db
        .exclusive_commitment()
        .id()
        .find(&commitment_id)
        .ok_or("Commitment not found")?;
    if actor_id != commitment.first_character_id && actor_id != commitment.second_character_id {
        return Err("Only a participant can cancel this wedding".into());
    }
    let minute = canonical_now(ctx, actor_id)?;
    if commitment.status != CommitmentStatus::Reserved {
        return Err(
            "Only a reserved wedding can be cancelled; end an active marriage instead".into(),
        );
    }
    if minute >= commitment.effective_minute {
        return Err("A wedding cannot be cancelled at or after its ceremony minute".into());
    }
    transition_commitment_terminal(
        ctx,
        commitment,
        CommitmentStatus::Cancelled,
        CommitmentTerminalReason::CancelledByParticipant,
        minute,
    );
    Ok(())
}

/// Scheduler hook for reservations which can no longer be serviced. Repeated
/// calls are no-ops and always release active uniqueness rows on first use.
pub fn expire_wedding_reservation(
    ctx: &ReducerContext,
    commitment_id: &str,
    minute: u64,
) -> Result<(), String> {
    let commitment = ctx
        .db
        .exclusive_commitment()
        .id()
        .find(&commitment_id.to_owned())
        .ok_or("Commitment not found")?;
    transition_commitment_terminal(
        ctx,
        commitment,
        CommitmentStatus::Expired,
        CommitmentTerminalReason::ReservationExpired,
        minute,
    );
    Ok(())
}

fn resolve_marriage(
    ctx: &ReducerContext,
    mut marriage: Marriage,
    status: MarriageStatus,
    minute: u64,
) {
    if marriage.status != MarriageStatus::Active {
        return;
    }
    marriage.status = status;
    marriage.resolved_minute = Some(minute);
    ctx.db.marriage().id().update(marriage.clone());
    for character_id in [marriage.first_character_id, marriage.second_character_id] {
        if ctx
            .db
            .household_member()
            .character_id()
            .find(character_id)
            .is_some_and(|member| member.household_id == marriage.household_id)
        {
            leave_household(ctx, character_id);
        }
        if ctx
            .db
            .residence_occupant()
            .character_id()
            .find(character_id)
            .is_some_and(|occupant| occupant.residence_character_id != character_id)
        {
            crate::residence::remove_occupant_at(ctx, character_id, minute);
        }
    }
    for (subject_id, related_id) in [
        (marriage.first_character_id, marriage.second_character_id),
        (marriage.second_character_id, marriage.first_character_id),
    ] {
        if ctx
            .db
            .marriage_participant()
            .character_id()
            .find(subject_id)
            .is_some_and(|row| row.marriage_id == marriage.id)
        {
            ctx.db
                .marriage_participant()
                .character_id()
                .delete(subject_id);
        }
        let id = kinship_id(subject_id, related_id, KinshipKind::Spouse);
        if ctx.db.character_kinship().id().find(&id).is_some() {
            ctx.db.character_kinship().id().delete(&id);
        }
    }
    if let Some(mut commitment) = ctx
        .db
        .exclusive_commitment()
        .id()
        .find(&marriage.commitment_id)
    {
        commitment.status = CommitmentStatus::Ended;
        commitment.resolved_minute = Some(minute);
        commitment.terminal_reason = Some(CommitmentTerminalReason::MarriageEnded);
        ctx.db
            .exclusive_commitment()
            .id()
            .update(commitment.clone());
        record_commitment_event(
            ctx,
            &commitment,
            CommitmentStatus::Ended,
            Some(CommitmentTerminalReason::MarriageEnded),
            minute,
        );
    }
}

#[reducer]
pub fn end_marriage(
    ctx: &ReducerContext,
    actor_id: u64,
    marriage_id: String,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, actor_id)?;
    let marriage = ctx
        .db
        .marriage()
        .id()
        .find(&marriage_id)
        .ok_or("Marriage not found")?;
    if actor_id != marriage.first_character_id && actor_id != marriage.second_character_id {
        return Err("Only a spouse can end this marriage".into());
    }
    let actor_minute = canonical_now(ctx, actor_id)?;
    if marriage.married_minute > actor_minute
        || marriage
            .resolved_minute
            .is_some_and(|resolved| resolved <= actor_minute)
    {
        return Err("Marriage is not effective at the actor's personal date".into());
    }
    resolve_marriage(ctx, marriage, MarriageStatus::Ended, actor_minute);
    Ok(())
}

pub fn settle_marriage_lifecycle_for_character(
    ctx: &ReducerContext,
    character_id: u64,
    minute: u64,
) {
    let Some(participant) = ctx
        .db
        .marriage_participant()
        .character_id()
        .find(character_id)
    else {
        return;
    };
    let Some(marriage) = ctx.db.marriage().id().find(&participant.marriage_id) else {
        return;
    };
    if marriage.married_minute > minute
        || marriage
            .resolved_minute
            .is_some_and(|resolved| resolved <= minute)
    {
        return;
    }
    let both_alive = [marriage.first_character_id, marriage.second_character_id]
        .into_iter()
        .all(|id| {
            ctx.db
                .character()
                .id()
                .find(id)
                .is_some_and(|row| row.alive)
        });
    if !both_alive {
        resolve_marriage(ctx, marriage, MarriageStatus::Widowed, minute);
    }
}

/// Internal-only scaffolding reducer.  Production NPC generation invokes the
/// same initializer; exposing no account-owned path prevents accidental NPC
/// time mutation through a player reducer.
#[reducer]
pub fn seed_npc_policy_for_development(
    ctx: &ReducerContext,
    character_id: u64,
    home_settlement_id: String,
    policy_seed: u64,
) -> Result<(), String> {
    if ctx.sender() != ctx.database_identity() {
        return Err("Only database administration can seed NPC policy".into());
    }
    initialize_npc_policy(ctx, character_id, home_settlement_id, policy_seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_id_is_order_independent() {
        assert_eq!(commitment_id(1, 9), commitment_id(9, 1));
    }

    #[test]
    fn engagement_is_one_year_notice() {
        assert_eq!(
            WEDDING_NOTICE_MINUTES,
            adventuresim_core::strategic_time::MINUTES_PER_YEAR
        );
    }

    #[test]
    fn npc_policy_uses_the_single_character_clock_and_central_lifecycle_hook() {
        let source = include_str!("relationship.rs");
        assert!(!source.contains("struct NpcPersonalTime"));
        assert!(!source.contains("npc_personal_time()"));
        let advancement = source
            .split("pub fn advance_npc_personal_time")
            .nth(1)
            .unwrap()
            .split("fn canonical_pair")
            .next()
            .unwrap();
        let clock_write = advancement
            .find("character_time().character_id().update(time)")
            .unwrap();
        assert!(
            clock_write
                < advancement
                    .find("settle_lifecycle_after_character_time_write")
                    .unwrap()
        );
        assert!(advancement.contains("target_minute < time.minutes"));
    }

    #[test]
    fn relationship_projection_is_effective_dated_by_personal_frontier() {
        let source = include_str!("relationship.rs");
        let projection = source
            .split("pub fn backend_character_relationship_statuses")
            .nth(1)
            .unwrap()
            .split("pub struct SocializingReceipt")
            .next()
            .unwrap();
        assert!(projection.contains("observer_minute"));
        assert!(projection.contains("marriage.married_minute <= observer_minute"));
        assert!(projection.contains("row.conceived_minute <= observer_minute"));
        assert!(projection.contains("receipt.attempted_minute <= observer_minute"));
        assert!(projection.contains("row.due_minute <= observer_minute"));
    }

    #[test]
    fn soft_scope_never_reads_or_synchronizes_the_target_clock() {
        let source = include_str!("relationship.rs");
        let guard = source
            .split("pub fn enforce_temporal_scope")
            .nth(1)
            .unwrap()
            .split("pub enum KinshipKind")
            .next()
            .unwrap();
        let soft = guard
            .split("TemporalScope::ActorLocal")
            .nth(1)
            .unwrap()
            .split("TemporalScope::NpcCanonical")
            .next()
            .unwrap();
        assert!(!soft.contains("canonical_now(ctx, target_id)"));
        assert!(!soft.contains("synchronize"));
    }

    #[test]
    fn exclusive_scope_uses_later_frontier_without_clock_synchronization() {
        let source = include_str!("relationship.rs");
        let guard = source
            .split("pub fn enforce_temporal_scope")
            .nth(1)
            .unwrap()
            .split("pub enum KinshipKind")
            .next()
            .unwrap();
        assert!(guard.contains("actor_minute.max(target_minute)"));
        assert!(!guard.contains("actor_minute != target_minute"));
        let wedding = source
            .split("pub fn settle_due_weddings")
            .nth(1)
            .unwrap()
            .split("pub fn settle_due_weddings_global")
            .next()
            .unwrap();
        assert!(wedding.contains("advance_npc_personal_time"));
        assert!(wedding.contains("player_lagging"));
    }

    #[test]
    fn seeded_family_contract_has_unique_roles_and_canonical_edges() {
        let source = include_str!("relationship.rs");
        let seed = source
            .split("pub fn ensure_seeded_family_households")
            .nth(1)
            .unwrap()
            .split("fn father_of")
            .next()
            .unwrap();
        assert!(seed.contains("residents.chunks(4)"));
        assert!(seed.contains("HouseholdRole::Head"));
        assert!(seed.contains("HouseholdRole::Spouse"));
        assert!(seed.contains("KinshipKind::Parent"));
        assert!(seed.contains("KinshipKind::Sibling"));
        assert!(seed.contains("character_personality().character_id().update"));
    }

    #[test]
    fn every_commitment_terminal_transition_releases_reservations_and_audits() {
        let source = include_str!("relationship.rs");
        let transition = source
            .split("fn transition_commitment_terminal")
            .nth(1)
            .unwrap()
            .split("/// Reserve two people")
            .next()
            .unwrap();
        assert!(transition.contains("exclusive_commitment_participant()"));
        assert!(transition.contains(".delete(character_id)"));
        assert!(transition.contains("record_commitment_event"));
        for status in ["Cancelled", "Expired", "Ended"] {
            assert!(source.contains(&format!("CommitmentStatus::{status}")));
        }
    }

    #[test]
    fn wedding_contract_revalidates_home_and_records_one_dowry_outcome() {
        let source = include_str!("relationship.rs");
        let wedding = source
            .split("pub fn settle_due_weddings")
            .nth(1)
            .unwrap()
            .split("pub fn establish_pregnancy")
            .next()
            .unwrap();
        assert!(wedding.contains("ParticipantUnderage"));
        assert!(wedding.contains("CeremonyLocationUnavailable"));
        assert!(wedding.contains("active_primary_residence"));
        assert!(wedding.contains("move_residence_occupant_internal"));
        assert!(wedding.contains("dowry_outcome()"));
        assert!(wedding.contains("commitment_id()"));
        assert!(wedding.contains("MarriageParticipant"));
    }

    #[test]
    fn discovery_attempts_are_daily_receipts_and_use_weaker_deception() {
        let source = include_str!("relationship.rs");
        let discovery = source
            .split("pub fn settle_secret_courtship_discovery_for_pair")
            .nth(1)
            .unwrap()
            .split("fn personality_disposition")
            .next()
            .unwrap();
        assert!(discovery.contains("{observer_id}:{day}"));
        assert!(discovery.contains("fold(f32::INFINITY, f32::min)"));
        assert!(discovery.contains("succeeded,"));
        assert!(discovery.contains("- 8.0"));
    }

    #[test]
    fn birth_uses_reserved_identity_and_constructs_age_zero() {
        let source = include_str!("relationship.rs");
        let birth = source
            .split("pub fn settle_due_births")
            .nth(1)
            .unwrap()
            .split("fn socializing_id")
            .next()
            .unwrap();
        assert!(birth.contains("pregnancy.reserved_child_id"));
        assert!(birth.contains("NpcLifeFacts {"));
        assert!(birth.contains("age_years: 0"));
        assert!(birth.contains("record_character_birth"));
        assert!(!birth.contains("child.age_years = 0"));
        assert!(birth.contains("active_pregnancy()"));
        assert!(birth.contains(".delete(pregnancy.mother_id)"));
    }

    #[test]
    fn spouse_leisure_is_simultaneous_conserved_and_idempotent() {
        let source = include_str!("relationship.rs");
        let settlement = source
            .split("fn settle_spouse_leisure_pair")
            .nth(1)
            .unwrap()
            .split("pub fn apply_spouse_leisure_conception")
            .next()
            .unwrap();
        assert!(settlement.contains("joint_leisure_minutes("));
        assert!(settlement.contains("conception_quantum_plan("));
        assert!(settlement.contains("spouse_leisure_overlap().id().find"));
        assert!(settlement.contains("conception_trial_receipt().id().find"));
        assert!(settlement.contains("refresh_spouse_pair_morale"));
    }

    #[test]
    fn spouse_morale_is_awarded_to_both_and_respects_combined_cap() {
        let source = include_str!("relationship.rs");
        let morale = source
            .split("fn refresh_spouse_pair_morale")
            .nth(1)
            .unwrap()
            .split("fn settle_spouse_leisure_pair")
            .next()
            .unwrap();
        assert!(morale.contains("for character_id in [first_id, second_id]"));
        assert!(morale.contains("LEISURE_MORALE_STACK_CAP_MILLI"));
        assert!(morale.contains("SPOUSE_LEISURE_MORALE_SPEC"));
    }

    #[test]
    fn wedding_resolution_uses_the_scheduled_effective_minute() {
        let source = include_str!("relationship.rs");
        let wedding = source
            .split("pub fn settle_due_weddings")
            .nth(1)
            .unwrap()
            .split("pub fn settle_due_weddings_global")
            .next()
            .unwrap();
        assert!(wedding.contains("let effective_minute = commitment.effective_minute"));
        assert!(!wedding.contains("WeddingCompleted,\n            now"));
    }

    #[test]
    fn secret_facade_is_daily_independent_and_stops_on_exposure() {
        let source = include_str!("relationship.rs");
        let daily = source
            .split("pub fn settle_secret_courtship_discovery_for_character")
            .nth(1)
            .unwrap()
            .split("fn personality_disposition")
            .next()
            .unwrap();
        assert!(daily.contains("next_discovery_day"));
        assert!(daily.contains("CourtshipStatus::Active"));
        assert!(daily.contains("settle_secret_courtship_discovery_for_pair"));
        let lifecycle = include_str!("time.rs")
            .split("settle_lifecycle_after_character_time_write")
            .nth(1)
            .unwrap()
            .split("pub fn advance_character_time")
            .next()
            .unwrap();
        assert!(lifecycle.contains("settle_secret_courtship_discovery_for_character"));
        let socializing = source
            .split("pub fn apply_scheduled_socializing")
            .nth(1)
            .unwrap()
            .split("pub fn settle_secret_courtship_discovery_for_pair")
            .next()
            .unwrap();
        assert!(!socializing.contains("settle_secret_courtship_discovery_for_pair"));
    }

    #[test]
    fn socializing_receipts_are_actor_day_cumulative_and_party_safe() {
        let source = include_str!("relationship.rs");
        let socializing = source
            .split("pub fn apply_scheduled_socializing")
            .nth(1)
            .unwrap()
            .split("pub fn settle_secret_courtship_discovery_for_pair")
            .next()
            .unwrap();
        assert!(source.contains("format!(\"socializing:{actor_id}:{day}\")"));
        assert!(socializing.contains("receipt.end_minute.max(start)"));
        assert!(socializing.contains("receipt.minutes.saturating_add(minutes)"));
        assert!(socializing.contains("apply_async_socializing_without_familiarity"));
        assert!(socializing.contains("select_daily_location_target"));
    }

    #[test]
    fn formal_route_uses_living_father_and_retry_is_explicit() {
        let source = include_str!("relationship.rs");
        let formal = source
            .split("pub fn begin_formal_courtship")
            .nth(1)
            .unwrap()
            .split("#[reducer]\npub fn begin_informal_courtship")
            .next()
            .unwrap();
        assert!(formal.contains("father_of(ctx, partner_id)"));
        let establishment = source
            .split("fn establish_courtship")
            .nth(1)
            .unwrap()
            .split("#[reducer]\npub fn begin_formal_courtship")
            .next()
            .unwrap();
        assert!(establishment.contains("active courtship of another kind"));
        assert!(establishment.contains("Ended courtship history is final"));
    }

    #[test]
    fn marriage_cleanup_releases_household_and_guest_occupancy() {
        let source = include_str!("relationship.rs");
        let resolution = source
            .split("fn resolve_marriage")
            .nth(1)
            .unwrap()
            .split("#[reducer]\npub fn end_marriage")
            .next()
            .unwrap();
        assert!(resolution.contains("leave_household"));
        assert!(resolution.contains("remove_occupant_at"));
        assert!(source.contains("#[unique]\n    pub character_id: u64"));
    }

    #[test]
    fn discovery_projection_is_gateway_and_observer_scoped() {
        let source = include_str!("relationship.rs");
        let projection = source
            .split("pub fn backend_courtship_discoveries")
            .nth(1)
            .unwrap()
            .split("pub struct SocializingReceipt")
            .next()
            .unwrap();
        assert!(projection.contains("is_strategic_gateway"));
        assert!(projection.contains("observer_character_id"));
        assert!(projection.contains("receipt.attempted_minute <= observer_minute"));
    }

    #[test]
    fn lifecycle_queue_has_explicit_cross_kind_order() {
        let mut events = [
            DueLifecycleEvent::Birth {
                effective_minute: 20,
                id: "birth-b".into(),
                mother_id: 2,
            },
            DueLifecycleEvent::Wedding {
                effective_minute: 20,
                id: "wedding-z".into(),
                participant_id: 3,
            },
            DueLifecycleEvent::Wedding {
                effective_minute: 10,
                id: "wedding-a".into(),
                participant_id: 1,
            },
        ];
        events.sort_by(|left, right| left.stable_key().cmp(&right.stable_key()));
        assert!(matches!(
            events[0],
            DueLifecycleEvent::Wedding {
                effective_minute: 10,
                ..
            }
        ));
        assert!(matches!(events[1], DueLifecycleEvent::Wedding { .. }));
        assert!(matches!(events[2], DueLifecycleEvent::Birth { .. }));
    }

    #[test]
    fn global_lifecycle_selection_is_bounded_and_poison_tolerant() {
        let source = include_str!("relationship.rs");
        let queue = source
            .split("pub fn settle_due_lifecycle_events_global")
            .nth(1)
            .unwrap()
            .split("fn socializing_id")
            .next()
            .unwrap();
        assert!(queue.contains(".effective_minute()"));
        assert!(queue.contains(".due_minute()"));
        assert!(queue.matches(".take(limit)").count() >= 2);
        assert!(queue.contains("due.truncate(limit)"));
        assert!(queue.contains("record_lifecycle_failure"));
        assert!(queue.contains("quarantine_invalid_birth"));
        assert!(queue.contains("validate_due_birth"));
        assert!(queue.contains("settle_due_births(ctx, mother_id, effective_minute)?"));
    }

    #[test]
    fn cancellation_only_applies_to_future_reserved_weddings() {
        let source = include_str!("relationship.rs");
        let cancel = source
            .split("pub fn cancel_wedding")
            .nth(1)
            .unwrap()
            .split("pub fn expire_wedding_reservation")
            .next()
            .unwrap();
        assert!(cancel.contains("commitment.status != CommitmentStatus::Reserved"));
        assert!(cancel.contains("minute >= commitment.effective_minute"));
    }

    #[test]
    fn born_child_projection_is_not_an_active_pregnancy_projection() {
        let source = include_str!("relationship.rs");
        let projection = source
            .split("pub fn backend_character_relationship_statuses")
            .nth(1)
            .unwrap()
            .split("pub fn backend_courtship_discoveries")
            .next()
            .unwrap();
        assert!(projection.contains("row.status == PregnancyStatus::Active"));
        assert!(projection.contains("row.status == PregnancyStatus::Born"));
        assert!(projection.contains("pregnancy_child_id: born_child_id"));
    }

    #[test]
    fn representative_soft_actions_guard_scope_without_target_clock_writes() {
        for source in [
            include_str!("strategic/dialogue_sessions.rs"),
            include_str!("organization.rs"),
            include_str!("strategic/inventory_trade.rs"),
            include_str!("residence.rs"),
        ] {
            assert!(source.contains("enforce_temporal_scope"));
        }
        let dialogue = include_str!("strategic/dialogue_sessions.rs");
        let guarded = dialogue
            .split("pub fn start_dialogue")
            .nth(1)
            .unwrap()
            .split("pub fn answer_dialogue_prompt")
            .next()
            .unwrap_or(dialogue);
        assert!(guarded.contains("TemporalScope::PairwiseSoft"));
        assert!(!guarded.contains("character_time().character_id().update"));
    }
}
