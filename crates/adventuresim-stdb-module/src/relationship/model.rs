// Owns the cohesive relationship, household, commitment, courtship, pregnancy,
// lifecycle-failure, and socializing schema.
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
            if ctx.db.character().id().find(target_id).is_none() {
                return Err("Exclusive scope requires an existing second participant".into());
            }
            // Commitments are facts in the settlement's canonical present.
            // The participants' subjective ages never need to match.
            crate::time::refresh_clock(ctx)
        }
    }
}

pub(crate) fn character_alive_at(ctx: &ReducerContext, character_id: u64, minute: u64) -> bool {
    ctx.db.character().id().find(character_id).is_some()
        && ctx
            .db
            .character_birth()
            .character_id()
            .find(character_id)
            .is_none_or(|birth| i128::from(birth.birth_minute) <= i128::from(minute))
        && ctx
            .db
            .character_death()
            .character_id()
            .find(character_id)
            .is_none_or(|death| death.strategic_minute > minute)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum KinshipKind {
    Parent,
    Child,
    Sibling,
    Spouse,
}

impl KinshipKind {
    const fn stable_id(self) -> &'static str {
        match self {
            Self::Parent => "Parent",
            Self::Child => "Child",
            Self::Sibling => "Sibling",
            Self::Spouse => "Spouse",
        }
    }
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

impl CommitmentStatus {
    const fn stable_id(self) -> &'static str {
        match self {
            Self::Reserved => "Reserved",
            Self::Fulfilled => "Fulfilled",
            Self::Cancelled => "Cancelled",
            Self::Expired => "Expired",
            Self::Ended => "Ended",
        }
    }
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

impl ExclusiveCommitment {
    fn parsed_state(&self) -> Result<adventuresim_core::strategic_state::CommitmentState, String> {
        use adventuresim_core::strategic_state::{
            FlatCommitmentReason as Reason, FlatCommitmentStatus as Flat,
        };
        adventuresim_core::strategic_state::CommitmentState::parse(
            match self.status {
                CommitmentStatus::Reserved => Flat::Reserved,
                CommitmentStatus::Fulfilled => Flat::Fulfilled,
                CommitmentStatus::Cancelled => Flat::Cancelled,
                CommitmentStatus::Expired => Flat::Expired,
                CommitmentStatus::Ended => Flat::Ended,
            },
            self.effective_minute,
            self.resolved_minute,
            self.terminal_reason.map(|reason| match reason {
                CommitmentTerminalReason::WeddingCompleted => Reason::WeddingCompleted,
                CommitmentTerminalReason::ParticipantDead => Reason::ParticipantDead,
                CommitmentTerminalReason::ParticipantUnderage => Reason::ParticipantUnderage,
                CommitmentTerminalReason::ResidenceUnavailable => Reason::ResidenceUnavailable,
                CommitmentTerminalReason::CeremonyLocationUnavailable => {
                    Reason::CeremonyLocationUnavailable
                }
                CommitmentTerminalReason::CancelledByParticipant => Reason::CancelledByParticipant,
                CommitmentTerminalReason::ReservationExpired => Reason::ReservationExpired,
                CommitmentTerminalReason::MarriageEnded => Reason::MarriageEnded,
            }),
        )
        .map_err(|error| error.to_string())
    }
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

impl Marriage {
    fn parsed_state(&self) -> Result<adventuresim_core::strategic_state::MarriageState, String> {
        use adventuresim_core::strategic_state::{FlatMarriageStatus as Flat, MarriageState};
        MarriageState::parse(
            match self.status {
                MarriageStatus::Active => Flat::Active,
                MarriageStatus::Widowed => Flat::Widowed,
                MarriageStatus::Ended => Flat::Ended,
            },
            self.resolved_minute,
        )
        .map_err(|error| error.to_string())
    }
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
    /// Formal approval is frozen at the shared courtship date. Later changes
    /// to the father's opinion, clock, or wealth cannot rewrite that history.
    pub approved_father_id: Option<u64>,
    pub planned_dowry_amount: u32,
    pub weaker_deception_baseline: f32,
    pub started_minute: u64,
    /// First relationship-day whose observer checks have not been resolved.
    pub next_discovery_day: u64,
    pub resolved_minute: Option<u64>,
    pub terminal_reason: Option<CourtshipTerminalReason>,
}

impl CourtshipRecord {
    fn parsed_state(
        &self,
    ) -> Result<
        (
            adventuresim_core::strategic_state::CourtshipRoute,
            adventuresim_core::strategic_state::CourtshipState,
        ),
        String,
    > {
        use adventuresim_core::strategic_state::{
            FlatCourtshipKind as Kind, FlatCourtshipSecrecyReason as Secrecy,
            FlatCourtshipStatus as Status, FlatCourtshipTerminalReason as Terminal,
        };
        adventuresim_core::strategic_state::parse_courtship(
            match self.kind {
                CourtshipKind::Formal => Kind::Formal,
                CourtshipKind::Informal => Kind::Informal,
            },
            self.secrecy_reason.map(|reason| match reason {
                CourtshipSecrecyReason::FatherDisapproval => Secrecy::FatherDisapproval,
                CourtshipSecrecyReason::FormalRouteUnavailable => Secrecy::FormalRouteUnavailable,
            }),
            self.approved_father_id,
            self.planned_dowry_amount,
            match self.status {
                CourtshipStatus::Active => Status::Active,
                CourtshipStatus::Exposed => Status::Exposed,
                CourtshipStatus::Ended => Status::Ended,
            },
            self.resolved_minute,
            self.terminal_reason.map(|reason| match reason {
                CourtshipTerminalReason::EngagementScheduled => Terminal::EngagementScheduled,
                CourtshipTerminalReason::EndedByParticipant => Terminal::EndedByParticipant,
                CourtshipTerminalReason::PartnerUnavailable => Terminal::PartnerUnavailable,
            }),
        )
        .map_err(|error| error.to_string())
    }
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

/// Observer eligibility and skill are frozen when the relationship starts.
/// Daily trials then depend only on authoritative personal frontiers.
#[derive(Clone, Debug)]
#[table(accessor = courtship_observer_baseline)]
pub struct CourtshipObserverBaseline {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub courtship_id: String,
    #[index(btree)]
    pub observer_id: u64,
    pub observer_insight: f32,
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
    pub birth_residence_holding_id: Option<String>,
    pub status: PregnancyStatus,
    pub birth_character_id: Option<u64>,
    pub resolved_minute: Option<u64>,
}

impl Pregnancy {
    fn parsed_state(&self) -> Result<adventuresim_core::strategic_state::PregnancyState, String> {
        use adventuresim_core::strategic_state::{FlatPregnancyStatus as Flat, PregnancyState};
        PregnancyState::parse(
            match self.status {
                PregnancyStatus::Active => Flat::Active,
                PregnancyStatus::Born => Flat::Born,
                PregnancyStatus::Ended => Flat::Ended,
            },
            self.birth_character_id,
            self.resolved_minute,
        )
        .map_err(|error| error.to_string())
    }
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

#[derive(Clone, Debug)]
#[table(accessor = dowry_escrow)]
pub struct DowryEscrow {
    #[primary_key]
    pub commitment_id: String,
    pub father_id: u64,
    pub amount: u32,
    pub reserved_minute: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum LifecycleEventKind {
    Wedding,
    Birth,
}

impl LifecycleEventKind {
    const fn stable_id(self) -> &'static str {
        match self {
            Self::Wedding => "Wedding",
            Self::Birth => "Birth",
        }
    }
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
    #[index(btree)]
    pub day: u64,
    pub start_minute: u64,
    pub end_minute: u64,
    pub minutes: u64,
}
