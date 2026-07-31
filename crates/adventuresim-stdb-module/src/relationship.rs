//! Canonical relationships are deliberately separate from asynchronous social
//! edges.  Affinity and familiarity are soft pairwise state; commitments,
//! kinship, pregnancy, and delayed ceremonies are globally exclusive facts.

use adventuresim_core::courtship::{
    ADULT_AGE_YEARS, CourtshipDisposition, FORMAL_COURTSHIP_AFFINITY,
    FORMAL_FATHER_APPROVAL_AFFINITY, GESTATION_MINUTES, WEDDING_NOTICE_MINUTES,
    deterministic_child_seeds, informal_affinity_threshold, succeeds_daily_trial,
};
use adventuresim_core::strategic_time::MINUTES_PER_DAY;
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::character::{character, character__view};
use crate::character_skills;
use crate::personality::{
    Courtship as PersonalityCourtship, Inclination, Presentation, Sex, character_personality,
};
use crate::residence::residence_occupant;
use crate::social::{CharacterAffinity, character_affinity};
use crate::strategic::strategic_gateway_authority__view;
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
            if actor_minute != target_minute {
                return Err(
                    "Exclusive canonical action requires synchronized personal dates".into(),
                );
            }
            Ok(actor_minute)
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
    #[index(btree)]
    pub character_id: u64,
    pub joined_minute: u64,
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
    pub due_minute: u64,
    pub reserved_child_id: u64,
    pub child_name_seed: u64,
    pub child_female: bool,
    pub child_home_seed: u64,
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
                let pregnancy = ctx
                    .db
                    .pregnancy()
                    .mother_id()
                    .filter(character.id)
                    .chain(ctx.db.pregnancy().father_id().filter(character.id))
                    .find(|row| {
                        (row.mother_id == character.id || row.father_id == character.id)
                            && row.conceived_minute <= observer_minute
                            && row
                                .resolved_minute
                                .is_none_or(|resolved| resolved > observer_minute)
                    });
                BackendCharacterRelationshipStatus {
                    character_id: character.id,
                    spouse_id,
                    courtship_partner_id,
                    courtship_kind,
                    courtship_exposed,
                    pregnancy_due_minute: pregnancy.as_ref().map(|row| row.due_minute),
                    pregnancy_child_id: pregnancy.and_then(|row| {
                        (row.due_minute <= observer_minute)
                            .then_some(row.birth_character_id)
                            .flatten()
                    }),
                }
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
        let Some(first) = ctx.db.character().id().find(commitment.first_character_id) else {
            transition_commitment_terminal(
                ctx,
                commitment,
                CommitmentStatus::Cancelled,
                CommitmentTerminalReason::ParticipantDead,
                now,
            );
            continue;
        };
        let Some(second) = ctx.db.character().id().find(commitment.second_character_id) else {
            transition_commitment_terminal(
                ctx,
                commitment,
                CommitmentStatus::Cancelled,
                CommitmentTerminalReason::ParticipantDead,
                now,
            );
            continue;
        };
        if !first.alive || !second.alive {
            transition_commitment_terminal(
                ctx,
                commitment,
                CommitmentStatus::Cancelled,
                CommitmentTerminalReason::ParticipantDead,
                now,
            );
            continue;
        }
        if first.age_years < ADULT_AGE_YEARS || second.age_years < ADULT_AGE_YEARS {
            transition_commitment_terminal(
                ctx,
                commitment,
                CommitmentStatus::Cancelled,
                CommitmentTerminalReason::ParticipantUnderage,
                now,
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
                now,
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
                now,
            );
            continue;
        };
        let household_id = format!("household:{}", commitment.id);
        if ctx.db.household().id().find(&household_id).is_none() {
            ctx.db.household().insert(Household {
                id: household_id.clone(),
                home_settlement_id: commitment.ceremony_settlement_id.clone(),
                created_minute: commitment.effective_minute,
            });
        }
        for character_id in [first.id, second.id] {
            let member_id = format!("household:{household_id}:{character_id}");
            if ctx.db.household_member().id().find(&member_id).is_none() {
                ctx.db.household_member().insert(HouseholdMember {
                    id: member_id,
                    household_id: household_id.clone(),
                    character_id,
                    joined_minute: commitment.effective_minute,
                });
            }
            crate::residence::admit_residence_occupant(
                ctx,
                residence_character_id,
                character_id,
                commitment.effective_minute,
            )?;
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
        if ctx
            .db
            .dowry_outcome()
            .commitment_id()
            .find(&commitment.id)
            .is_none()
        {
            let (father_id, amount, outcome) = if !formal {
                (None, 0, DowryOutcomeKind::NotFormal)
            } else if let Some(father) = father_of(ctx, bride_id) {
                let amount = formal_dowry_amount(crate::item::personal_currency_total(ctx, father));
                if amount == 0 {
                    (Some(father), 0, DowryOutcomeKind::NoDowry)
                } else if crate::item::consume_personal_currency(ctx, father, amount.into())
                    .is_err()
                {
                    (Some(father), amount, DowryOutcomeKind::InsufficientFunds)
                } else {
                    crate::item::credit_personal_currency(
                        ctx,
                        recipient_id,
                        &commitment.ceremony_settlement_id,
                        amount,
                    )?;
                    (Some(father), amount, DowryOutcomeKind::Paid)
                }
            } else {
                (None, 0, DowryOutcomeKind::FatherUnavailable)
            };
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
            now,
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
    let home = ctx
        .db
        .character()
        .id()
        .find(mother_id)
        .and_then(|mother| mother.current_settlement_id)
        .or_else(|| {
            ctx.db
                .character()
                .id()
                .find(father_id)
                .and_then(|father| father.current_settlement_id)
        })
        .ok_or("Pregnancy requires a parent home settlement")?;
    let seeds = deterministic_child_seeds(
        &mother_id.to_string(),
        &father_id.to_string(),
        ordinal,
        due_minute,
        &home,
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
        status: PregnancyStatus::Active,
        birth_character_id: None,
        resolved_minute: None,
    };
    ctx.db.pregnancy().insert(pregnancy.clone());
    ctx.db.active_pregnancy().insert(ActivePregnancy {
        mother_id,
        pregnancy_id: id,
    });
    Ok(pregnancy)
}

/// Deterministic conception gate for qualifying spouse Leisure.  It is keyed
/// to the calendar day, making a month advanced in one call equivalent to the
/// same month advanced day by day.  No fertility, contraception, miscarriage,
/// or complications are modeled in this first pass.
pub fn attempt_spouse_conception(
    ctx: &ReducerContext,
    first_id: u64,
    second_id: u64,
    day: u64,
) -> Result<Option<Pregnancy>, String> {
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
    if !first.alive
        || !second.alive
        || first.age_years < ADULT_AGE_YEARS
        || second.age_years < ADULT_AGE_YEARS
        || first.current_settlement_id != second.current_settlement_id
        || !ctx.db.character_kinship().iter().any(|edge| {
            edge.subject_id == first_id
                && edge.related_id == second_id
                && edge.kind == KinshipKind::Spouse
        })
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
    if ctx
        .db
        .active_pregnancy()
        .mother_id()
        .find(mother_id)
        .is_some()
    {
        return Ok(None);
    }
    let entropy = ((first_id ^ second_id ^ day.rotate_left(11)) % 10_000) as u16;
    // Three percent per qualifying daily spouse-Leisure event; tuning is kept
    // local to this deterministic gate rather than embedded in UI code.
    if !succeeds_daily_trial(entropy, 300) {
        return Ok(None);
    }
    establish_pregnancy(
        ctx,
        mother_id,
        father_id,
        day.saturating_mul(MINUTES_PER_DAY),
    )
    .map(Some)
}

pub fn apply_spouse_leisure_conception(
    ctx: &ReducerContext,
    character_id: u64,
    interval_start: u64,
    interval_end: u64,
    qualifying_leisure_minutes: u64,
) -> Result<(), String> {
    if qualifying_leisure_minutes == 0 || interval_end <= interval_start {
        return Ok(());
    }
    let Some(spouse_id) = ctx.db.character_kinship().iter().find_map(|edge| {
        (edge.subject_id == character_id && edge.kind == KinshipKind::Spouse)
            .then_some(edge.related_id)
    }) else {
        return Ok(());
    };
    for day in
        (interval_start / MINUTES_PER_DAY)..=(interval_end.saturating_sub(1) / MINUTES_PER_DAY)
    {
        let _ = attempt_spouse_conception(ctx, character_id, spouse_id, day)?;
    }
    Ok(())
}

/// Colocated spouses refresh a durable morale benefit from qualifying Leisure.
/// The source is pair-stable, so repeated leisure refreshes rather than stacks
/// unbounded events and remains independent of a residence comfort bonus.
pub fn apply_spouse_leisure_morale(
    ctx: &ReducerContext,
    character_id: u64,
    interval_end: u64,
    qualifying_leisure_minutes: u64,
) -> Result<(), String> {
    if qualifying_leisure_minutes == 0 {
        return Ok(());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let Some(spouse_id) = ctx.db.character_kinship().iter().find_map(|edge| {
        (edge.subject_id == character_id && edge.kind == KinshipKind::Spouse)
            .then_some(edge.related_id)
    }) else {
        return Ok(());
    };
    let spouse = ctx
        .db
        .character()
        .id()
        .find(spouse_id)
        .ok_or("Spouse not found")?;
    if !character.alive
        || !spouse.alive
        || character.current_settlement_id != spouse.current_settlement_id
    {
        return Ok(());
    }
    let source = format!(
        "spouse-leisure:{}:{}",
        character_id.min(spouse_id),
        character_id.max(spouse_id)
    );
    crate::condition::upsert_refreshable_morale_event_at_without_refresh(
        ctx,
        character_id,
        "spouse_leisure",
        2.0,
        interval_end,
        &source,
    )?;
    crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
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
        let settlement_id = mother
            .current_settlement_id
            .clone()
            .or(father.current_settlement_id.clone())
            .ok_or("Birth requires a home settlement")?;
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
                mode: crate::character::CharacterCreationMode::PersistentNpc,
                create_solo_party: false,
                stable_seed: pregnancy.child_name_seed,
                initial_time_minute: Some(pregnancy.due_minute),
            },
            None,
            Some(&newborn_life),
        )?;
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
        initialize_npc_policy(ctx, child_id, settlement_id, pregnancy.child_home_seed)?;
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
            .filter(mother.id)
            .next()
            .or_else(|| {
                ctx.db
                    .household_member()
                    .character_id()
                    .filter(father.id)
                    .next()
            })
        {
            let id = format!("household:{}:{child_id}", household_member.household_id);
            if ctx.db.household_member().id().find(&id).is_none() {
                ctx.db.household_member().insert(HouseholdMember {
                    id,
                    household_id: household_member.household_id,
                    character_id: child_id,
                    joined_minute: pregnancy.due_minute,
                });
            }
        }
        if let Some(residence_character_id) = [mother.id, father.id].into_iter().find_map(|id| {
            ctx.db
                .residence_occupant()
                .character_id()
                .find(id)
                .map(|occupant| occupant.residence_character_id)
        }) {
            crate::residence::admit_residence_occupant(
                ctx,
                residence_character_id,
                child_id,
                pregnancy.due_minute,
            )?;
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

fn socializing_id(actor_id: u64, start_minute: u64, end_minute: u64) -> String {
    format!("socializing:{actor_id}:{start_minute}:{end_minute}")
}

fn active_romantic_partner(ctx: &ReducerContext, actor_id: u64) -> Option<u64> {
    let courtship = ctx.db.courtship().iter().find(|row| {
        row.status != CourtshipStatus::Ended
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
    let choose = |mut candidates: Vec<u64>| {
        candidates.sort_unstable();
        candidates.dedup();
        (!candidates.is_empty())
            .then(|| candidates[((actor_id ^ day.rotate_left(17)) as usize) % candidates.len()])
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
        let minutes = allocation(end).saturating_sub(allocation(start));
        if minutes == 0 {
            continue;
        }
        let id = socializing_id(actor_id, start, end);
        if ctx.db.socializing_receipt().id().find(&id).is_some() {
            continue;
        }
        let Some(target_id) = socializing_target(ctx, actor_id, day) else {
            continue;
        };
        let _ =
            enforce_temporal_scope(ctx, actor_id, Some(target_id), TemporalScope::PairwiseSoft)?;
        crate::social::apply_async_socializing(ctx, actor_id, target_id, minutes)?;
        settle_secret_courtship_discovery_for_pair(ctx, actor_id, target_id, day)?;
        ctx.db.socializing_receipt().insert(SocializingReceipt {
            id,
            actor_id,
            target_id,
            start_minute: start,
            end_minute: end,
            minutes,
        });
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
    if courtship.kind != CourtshipKind::Informal || courtship.status == CourtshipStatus::Ended {
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
            || observer.age_years < ADULT_AGE_YEARS
            || (observer.current_settlement_id != first_person.current_settlement_id
                && observer.current_settlement_id != second_person.current_settlement_id)
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
        let attempted_minute = day.saturating_mul(MINUTES_PER_DAY);
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
    if !suitor.alive
        || !partner.alive
        || suitor.age_years < ADULT_AGE_YEARS
        || partner.age_years < ADULT_AGE_YEARS
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
    enforce_temporal_scope(
        ctx,
        suitor_id,
        Some(partner_id),
        TemporalScope::ExclusiveShared,
    )
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
    if ctx.db.courtship().id().find(&id).is_some() {
        return Ok(());
    }
    ctx.db.courtship().insert(CourtshipRecord {
        id,
        first_character_id,
        second_character_id,
        kind,
        status: CourtshipStatus::Active,
        secrecy_reason,
        started_minute: minute,
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
    let father = ctx
        .db
        .character_kinship()
        .iter()
        .find_map(|edge| {
            (edge.subject_id == partner_id && edge.kind == KinshipKind::Parent)
                .then(|| {
                    ctx.db
                        .character_personality()
                        .character_id()
                        .find(edge.related_id)
                        .filter(|p| p.sex == Sex::Male)
                        .map(|_| edge.related_id)
                })
                .flatten()
        })
        .ok_or("Formal courtship requires a known living father")?;
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
    resolve_marriage(
        ctx,
        marriage,
        MarriageStatus::Ended,
        canonical_now(ctx, actor_id)?,
    );
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
        assert!(wedding.contains("admit_residence_occupant"));
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
        assert!(!birth.contains("child.age_years = 0"));
        assert!(birth.contains("active_pregnancy()"));
        assert!(birth.contains(".delete(pregnancy.mother_id)"));
    }
}
