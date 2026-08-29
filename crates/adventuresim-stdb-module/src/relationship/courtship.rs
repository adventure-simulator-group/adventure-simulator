// Owns courtship validation, establishment, reducer boundaries, and rejection policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CourtshipPairError {
    Rejected(CourtshipRejection),
    InvalidState(String),
}

impl CourtshipPairError {
    fn rejected(code: CourtshipRejectionCode, detail: impl Into<String>) -> Self {
        Self::Rejected(CourtshipRejection::new(code, detail))
    }

    fn rejection_code(&self) -> Option<CourtshipRejectionCode> {
        match self {
            Self::Rejected(rejection) => Some(rejection.code),
            Self::InvalidState(_) => None,
        }
    }

    pub(crate) fn into_reducer_error(self) -> String {
        match self {
            Self::Rejected(rejection) => encode_courtship_rejection(&rejection),
            Self::InvalidState(detail) => detail,
        }
    }
}

impl From<String> for CourtshipPairError {
    fn from(detail: String) -> Self {
        Self::InvalidState(detail)
    }
}

impl From<&str> for CourtshipPairError {
    fn from(detail: &str) -> Self {
        Self::InvalidState(detail.to_owned())
    }
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
) -> Result<u64, CourtshipPairError> {
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
        return Err(CourtshipPairError::rejected(
            CourtshipRejectionCode::IneligibleCharacter,
            "Courtship requires two living adult characters",
        ));
    }
    if suitor.current_settlement_id != partner.current_settlement_id {
        return Err(CourtshipPairError::rejected(
            CourtshipRejectionCode::CoLocation,
            "Courtship requires co-location",
        ));
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
        return Err(CourtshipPairError::rejected(
            CourtshipRejectionCode::MutualAttraction,
            "This pair does not have mutual attraction",
        ));
    }
    let (first, second) = canonical_pair(suitor_id, partner_id);
    let permitted_courtship_id = format!("courtship:{first}:{second}");
    if relationship_conflicts_at(
        ctx,
        suitor_id,
        effective_minute,
        Some(&permitted_courtship_id),
    ) || relationship_conflicts_at(
        ctx,
        partner_id,
        effective_minute,
        Some(&permitted_courtship_id),
    ) {
        return Err(CourtshipPairError::rejected(
            CourtshipRejectionCode::ExclusiveCommitment,
            "An exclusive romantic commitment prevents new courtship",
        ));
    }
    if ctx
        .db
        .character_kinship()
        .iter()
        .any(|edge| edge.subject_id == suitor_id && edge.related_id == partner_id)
    {
        return Err(CourtshipPairError::rejected(
            CourtshipRejectionCode::CloseRelative,
            "Close relatives cannot court",
        ));
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
) -> Result<(), CourtshipPairError> {
    let (first_character_id, second_character_id) = canonical_pair(suitor_id, partner_id);
    let id = format!("courtship:{first_character_id}:{second_character_id}");
    if let Some(existing) = ctx.db.courtship().id().find(&id) {
        existing.parsed_state()?;
        return match (existing.status, existing.kind == kind) {
            (CourtshipStatus::Active | CourtshipStatus::Exposed, true) => Ok(()),
            (CourtshipStatus::Active | CourtshipStatus::Exposed, false) => {
                Err(CourtshipPairError::rejected(
                    CourtshipRejectionCode::ExclusiveCommitment,
                    "This pair already has an active courtship of another kind",
                ))
            }
            (CourtshipStatus::Ended, _) => {
                Err("Ended courtship history is final for this pair".into())
            }
        };
    }
    let (approved_father_id, planned_dowry_amount) = if kind == CourtshipKind::Formal {
        let father = father_of_at(ctx, partner_id, minute)
            .map_err(|detail| {
                CourtshipPairError::rejected(CourtshipRejectionCode::FatherApproval, detail)
            })?
            .ok_or_else(|| {
                CourtshipPairError::rejected(
                    CourtshipRejectionCode::FatherApproval,
                    "Formal courtship requires a known living father",
                )
            })?;
        (
            Some(father),
            formal_dowry_amount(crate::item::personal_currency_total(ctx, father)),
        )
    } else {
        (None, 0)
    };
    let weaker_deception_baseline = [first_character_id, second_character_id]
        .into_iter()
        .filter_map(|character_id| ctx.db.character_skills().character_id().find(character_id))
        .map(|skills| skills.deception_hours.sqrt())
        .fold(f32::INFINITY, f32::min);
    let weaker_deception_baseline = if weaker_deception_baseline.is_finite() {
        weaker_deception_baseline
    } else {
        0.0
    };
    ctx.db.courtship().insert(CourtshipRecord {
        id: id.clone(),
        first_character_id,
        second_character_id,
        kind,
        status: CourtshipStatus::Active,
        secrecy_reason,
        approved_father_id,
        planned_dowry_amount,
        weaker_deception_baseline,
        started_minute: minute,
        next_discovery_day: minute / MINUTES_PER_DAY,
        resolved_minute: None,
        terminal_reason: None,
    });
    if kind == CourtshipKind::Informal {
        let pair_settlement = ctx
            .db
            .character()
            .id()
            .find(first_character_id)
            .and_then(|character| character.current_settlement_id);
        let mut observer_ids = ctx
            .db
            .character_kinship()
            .iter()
            .filter(|edge| {
                (edge.subject_id == first_character_id || edge.subject_id == second_character_id)
                    && matches!(edge.kind, KinshipKind::Parent | KinshipKind::Sibling)
                    && edge.established_minute <= minute
            })
            .map(|edge| edge.related_id)
            .collect::<Vec<_>>();
        observer_ids.sort_unstable();
        observer_ids.dedup();
        for observer_id in observer_ids {
            let Some(observer) = ctx.db.character().id().find(observer_id) else {
                continue;
            };
            if !character_alive_at(ctx, observer_id, minute)
                || effective_age_years(ctx, observer_id, minute).unwrap_or(observer.age_years)
                    < ADULT_AGE_YEARS
                || observer.current_settlement_id != pair_settlement
            {
                continue;
            }
            let observer_insight = ctx
                .db
                .character_skills()
                .character_id()
                .find(observer_id)
                .map_or(0.0, |skills| skills.insight_hours.sqrt());
            ctx.db
                .courtship_observer_baseline()
                .insert(CourtshipObserverBaseline {
                    id: format!("courtship-observer:{id}:{observer_id}"),
                    courtship_id: id.clone(),
                    observer_id,
                    observer_insight,
                });
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NpcCourtshipOutcome {
    Formal,
    Informal,
    Ineligible,
}

/// Scheduler-only NPC-to-NPC courtship and engagement transaction. Expected
/// social ineligibility is a no-op outcome; missing canonical components or a
/// broken invariant aborts the scheduler reducer.
pub(crate) fn establish_npc_courtship_and_wedding(
    ctx: &ReducerContext,
    suitor_id: u64,
    partner_id: u64,
) -> Result<NpcCourtshipOutcome, CourtshipPairError> {
    if suitor_id == partner_id
        || ctx.db.npc_policy().character_id().find(suitor_id).is_none()
        || ctx
            .db
            .npc_policy()
            .character_id()
            .find(partner_id)
            .is_none()
    {
        return Ok(NpcCourtshipOutcome::Ineligible);
    }
    let suitor = ctx
        .db
        .character()
        .id()
        .find(suitor_id)
        .ok_or("NPC suitor character not found")?;
    let partner = ctx
        .db
        .character()
        .id()
        .find(partner_id)
        .ok_or("NPC partner character not found")?;
    let suitor_time = canonical_now(ctx, suitor_id)?;
    let partner_time = canonical_now(ctx, partner_id)?;
    let effective_minute = suitor_time.max(partner_time);
    if !suitor.alive
        || !partner.alive
        || effective_age_years(ctx, suitor_id, effective_minute).unwrap_or(suitor.age_years)
            < ADULT_AGE_YEARS
        || effective_age_years(ctx, partner_id, effective_minute).unwrap_or(partner.age_years)
            < ADULT_AGE_YEARS
        || suitor.current_settlement_id.is_none()
        || suitor.current_settlement_id != partner.current_settlement_id
    {
        return Ok(NpcCourtshipOutcome::Ineligible);
    }
    let suitor_personality = ctx
        .db
        .character_personality()
        .character_id()
        .find(suitor_id)
        .ok_or("NPC suitor personality not found")?;
    let partner_personality = ctx
        .db
        .character_personality()
        .character_id()
        .find(partner_id)
        .ok_or("NPC partner personality not found")?;
    if !inclination_accepts(
        suitor_personality.inclination,
        partner_personality.presentation,
    ) || !inclination_accepts(
        partner_personality.inclination,
        suitor_personality.presentation,
    ) {
        return Ok(NpcCourtshipOutcome::Ineligible);
    }
    if relationship_conflicts_at(ctx, suitor_id, effective_minute, None)
        || relationship_conflicts_at(ctx, partner_id, effective_minute, None)
        || ctx.db.character_kinship().iter().any(|edge| {
            (edge.subject_id == suitor_id && edge.related_id == partner_id)
                || (edge.subject_id == partner_id && edge.related_id == suitor_id)
        })
    {
        return Ok(NpcCourtshipOutcome::Ineligible);
    }
    let (first, second) = canonical_pair(suitor_id, partner_id);
    let courtship_id = format!("courtship:{first}:{second}");
    if ctx.db.courtship().id().find(&courtship_id).is_some() {
        return Ok(NpcCourtshipOutcome::Ineligible);
    }

    let Some(partner_affinity) = affinity_at(ctx, partner_id, suitor_id, effective_minute) else {
        return Ok(NpcCourtshipOutcome::Ineligible);
    };
    let formal_pair = suitor_personality.sex == Sex::Male && partner_personality.sex == Sex::Female;
    let living_father = match father_of_at(ctx, partner_id, effective_minute) {
        Ok(father) => father,
        Err(_) => return Ok(NpcCourtshipOutcome::Ineligible),
    };
    let father_approves = living_father.is_some_and(|father| {
        affinity_at(ctx, father, suitor_id, effective_minute)
            .is_some_and(|affinity| affinity >= FORMAL_FATHER_APPROVAL_AFFINITY)
    });
    let route = adventuresim_core::npc_policy::npc_courtship_route(
        adventuresim_core::npc_policy::NpcCourtshipEligibility {
            both_npc: true,
            co_located: true,
            living_adults: true,
            mutually_attracted: true,
            nonkin: true,
            conflict_free: true,
            formal_pair,
            father_approves,
            formal_affinity_met: partner_affinity >= FORMAL_COURTSHIP_AFFINITY,
            informal_affinity_met: partner_affinity
                >= informal_affinity_threshold(personality_disposition(
                    partner_personality.courtship,
                )),
        },
    );
    let (kind, secrecy_reason, outcome) = match route {
        adventuresim_core::npc_policy::NpcCourtshipRoute::Formal => {
            (CourtshipKind::Formal, None, NpcCourtshipOutcome::Formal)
        }
        adventuresim_core::npc_policy::NpcCourtshipRoute::Informal => {
            let reason = if formal_pair && living_father.is_some() {
                CourtshipSecrecyReason::FatherDisapproval
            } else {
                CourtshipSecrecyReason::FormalRouteUnavailable
            };
            (
                CourtshipKind::Informal,
                Some(reason),
                NpcCourtshipOutcome::Informal,
            )
        }
        adventuresim_core::npc_policy::NpcCourtshipRoute::Ineligible => {
            return Ok(NpcCourtshipOutcome::Ineligible);
        }
    };

    // Reuse the complete shared validator immediately before the atomic
    // writes. Every expected rejection above has already become a no-op, so a
    // failure here means canonical infrastructure changed underneath policy.
    let validated_minute = validate_canonical_courtship_pair(ctx, suitor_id, partner_id)?;
    establish_courtship(
        ctx,
        suitor_id,
        partner_id,
        kind,
        secrecy_reason,
        validated_minute,
    )?;
    reserve_wedding(ctx, first, second, validated_minute)?;
    Ok(outcome)
}

#[reducer]
pub fn begin_formal_courtship(
    ctx: &ReducerContext,
    suitor_id: u64,
    partner_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, suitor_id)?;
    let minute = match validate_canonical_courtship_pair(ctx, suitor_id, partner_id) {
        Ok(minute) => minute,
        Err(error)
            if error.rejection_code() == Some(CourtshipRejectionCode::ExclusiveCommitment) =>
        {
            return Ok(());
        }
        Err(error) => return Err(error.into_reducer_error()),
    };
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
        return Err(CourtshipPairError::rejected(
            CourtshipRejectionCode::FormalRoute,
            "Formal courtship currently requires a man suitor and woman partner",
        )
        .into_reducer_error());
    }
    if affinity_at(ctx, partner_id, suitor_id, minute)
        .is_none_or(|affinity| affinity < FORMAL_COURTSHIP_AFFINITY)
    {
        return Err(CourtshipPairError::rejected(
            CourtshipRejectionCode::Affinity,
            "The prospective partner does not yet have enough affinity",
        )
        .into_reducer_error());
    }
    let father = father_of_at(ctx, partner_id, minute)
        .map_err(|detail| {
            CourtshipPairError::rejected(CourtshipRejectionCode::FatherApproval, detail)
                .into_reducer_error()
        })?
        .ok_or_else(|| {
            CourtshipPairError::rejected(
                CourtshipRejectionCode::FatherApproval,
                "Formal courtship requires a known living father",
            )
            .into_reducer_error()
        })?;
    if affinity_at(ctx, father, suitor_id, minute)
        .is_none_or(|affinity| affinity < FORMAL_FATHER_APPROVAL_AFFINITY)
    {
        return Err(CourtshipPairError::rejected(
            CourtshipRejectionCode::FatherApproval,
            "Her father does not approve of this suitor",
        )
        .into_reducer_error());
    }
    establish_courtship(
        ctx,
        suitor_id,
        partner_id,
        CourtshipKind::Formal,
        None,
        minute,
    )
    .map_err(CourtshipPairError::into_reducer_error)
}

/// Prepare a compatible pair for browser-driven courtship testing without
/// weakening the normal affinity and family-approval rules.
///
/// Only the registered strategic gateway can call this reducer. Production
/// gameplay never invokes it; developer tooling uses it to reach the
/// year-long marriage and child lifecycle in a bounded test session.
#[reducer]
pub fn prepare_development_courtship(
    ctx: &ReducerContext,
    suitor_id: u64,
    partner_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    let settlement_id = ctx
        .db
        .character()
        .id()
        .find(suitor_id)
        .and_then(|character| character.current_settlement_id)
        .ok_or("Development courtship requires a current settlement")?;
    crate::item::credit_personal_currency(ctx, suitor_id, &settlement_id, 10_000)?;
    let minute = validate_canonical_courtship_pair(ctx, suitor_id, partner_id)
        .map_err(CourtshipPairError::into_reducer_error)?;
    crate::social::put_affinity_at(ctx, partner_id, suitor_id, 100.0, minute);
    if let Some(father_id) = father_of_at(ctx, partner_id, minute)? {
        crate::social::put_affinity_at(ctx, father_id, suitor_id, 100.0, minute);
    }
    Ok(())
}

#[reducer]
pub fn begin_informal_courtship(
    ctx: &ReducerContext,
    suitor_id: u64,
    partner_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, suitor_id)?;
    let minute = validate_canonical_courtship_pair(ctx, suitor_id, partner_id)
        .map_err(CourtshipPairError::into_reducer_error)?;
    let partner = ctx
        .db
        .character_personality()
        .character_id()
        .find(partner_id)
        .ok_or("Partner personality not found")?;
    if affinity_at(ctx, partner_id, suitor_id, minute).is_none_or(|affinity| {
        affinity < informal_affinity_threshold(personality_disposition(partner.courtship))
    }) {
        return Err(CourtshipPairError::rejected(
            CourtshipRejectionCode::Affinity,
            "The prospective partner does not yet have enough affinity for informal courtship",
        )
        .into_reducer_error());
    }
    let suitor_personality = ctx
        .db
        .character_personality()
        .character_id()
        .find(suitor_id)
        .ok_or("Suitor personality not found")?;
    let formal_pair = suitor_personality.sex == Sex::Male && partner.sex == Sex::Female;
    let living_father = father_of_at(ctx, partner_id, minute).map_err(|detail| {
        CourtshipPairError::rejected(CourtshipRejectionCode::FatherApproval, detail)
            .into_reducer_error()
    })?;
    let father_approves = living_father.is_some_and(|father| {
        affinity_at(ctx, father, suitor_id, minute)
            .is_some_and(|affinity| affinity >= FORMAL_FATHER_APPROVAL_AFFINITY)
    });
    if formal_pair && father_approves {
        return Err(CourtshipPairError::rejected(
            CourtshipRejectionCode::FormalRoute,
            "Her father's approval makes the formal route available",
        )
        .into_reducer_error());
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
    .map_err(CourtshipPairError::into_reducer_error)
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
    let minute = validate_canonical_courtship_pair(ctx, first_character_id, second_character_id)
        .map_err(CourtshipPairError::into_reducer_error)?;
    let (first, second) = canonical_pair(first_character_id, second_character_id);
    let courtship_id = format!("courtship:{first}:{second}");
    if !ctx
        .db
        .courtship()
        .id()
        .find(&courtship_id)
        .is_some_and(|courtship| courtship.status != CourtshipStatus::Ended)
    {
        return Err(CourtshipPairError::rejected(
            CourtshipRejectionCode::ActiveCourtshipRequired,
            "A wedding requires an active courtship",
        )
        .into_reducer_error());
    }
    reserve_wedding(ctx, first, second, minute)
        .map(|_| ())
        .map_err(CourtshipPairError::into_reducer_error)
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
    commitment.parsed_state()?;
    if actor_id != commitment.first_character_id && actor_id != commitment.second_character_id {
        return Err("Only a participant can cancel this wedding".into());
    }
    let minute = crate::time::refresh_clock(ctx)?;
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
    )?;
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
        .find(commitment_id.to_owned())
        .ok_or("Commitment not found")?;
    commitment.parsed_state()?;
    transition_commitment_terminal(
        ctx,
        commitment,
        CommitmentStatus::Expired,
        CommitmentTerminalReason::ReservationExpired,
        minute,
    )?;
    Ok(())
}
