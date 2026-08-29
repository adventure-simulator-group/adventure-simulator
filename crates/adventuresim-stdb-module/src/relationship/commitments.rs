// Owns exclusive commitment identity, history, reservations, and terminal transitions.
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
    let id = format!(
        "commitment-event:{}:{minute}:{}",
        commitment.id,
        status.stable_id()
    );
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
) -> Result<ExclusiveCommitment, String> {
    commitment.parsed_state()?;
    if commitment.status != CommitmentStatus::Reserved {
        return Ok(commitment);
    }
    if status != CommitmentStatus::Fulfilled
        && let Some(escrow) = ctx.db.dowry_escrow().commitment_id().find(&commitment.id)
    {
        crate::item::credit_personal_currency(
            ctx,
            escrow.father_id,
            &commitment.ceremony_settlement_id,
            escrow.amount,
        )?;
        ctx.db.dowry_escrow().commitment_id().delete(&commitment.id);
    }
    if reason == CommitmentTerminalReason::ParticipantDead {
        let courtship_id = format!(
            "courtship:{}:{}",
            commitment
                .first_character_id
                .min(commitment.second_character_id),
            commitment
                .first_character_id
                .max(commitment.second_character_id)
        );
        if let Some(mut courtship) = ctx.db.courtship().id().find(&courtship_id)
            && courtship.status != CourtshipStatus::Ended
        {
            courtship.status = CourtshipStatus::Ended;
            courtship.resolved_minute = Some(minute);
            courtship.terminal_reason = Some(CourtshipTerminalReason::PartnerUnavailable);
            ctx.db.courtship().id().update(courtship);
        }
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
    Ok(commitment)
}

/// Close relationship state whose subject can no longer reach a future
/// lifecycle boundary. Death freezes CharacterTime, so cleanup must happen at
/// the death transaction rather than waiting for the wedding/birth queues.
pub(crate) fn settle_relationship_lifecycle_for_death(
    ctx: &ReducerContext,
    character_id: u64,
    death_minute: u64,
) -> Result<(), String> {
    let mut commitments = ctx
        .db
        .exclusive_commitment()
        .iter()
        .filter(|commitment| {
            commitment.status == CommitmentStatus::Reserved
                && (commitment.first_character_id == character_id
                    || commitment.second_character_id == character_id)
        })
        .collect::<Vec<_>>();
    commitments.sort_by(|left, right| {
        (left.effective_minute, left.id.as_str()).cmp(&(right.effective_minute, right.id.as_str()))
    });
    for commitment in commitments {
        transition_commitment_terminal(
            ctx,
            commitment,
            CommitmentStatus::Cancelled,
            CommitmentTerminalReason::ParticipantDead,
            death_minute,
        )?;
    }

    let mut courtships = ctx
        .db
        .courtship()
        .iter()
        .filter(|courtship| {
            courtship.status != CourtshipStatus::Ended
                && (courtship.first_character_id == character_id
                    || courtship.second_character_id == character_id)
        })
        .collect::<Vec<_>>();
    courtships.sort_by(|left, right| left.id.cmp(&right.id));
    for mut courtship in courtships {
        courtship.parsed_state()?;
        courtship.status = CourtshipStatus::Ended;
        courtship.resolved_minute = Some(death_minute);
        courtship.terminal_reason = Some(CourtshipTerminalReason::PartnerUnavailable);
        ctx.db.courtship().id().update(courtship);
    }

    let mut pregnancies = ctx
        .db
        .pregnancy()
        .mother_id()
        .filter(character_id)
        .filter(|pregnancy| pregnancy.status == PregnancyStatus::Active)
        .collect::<Vec<_>>();
    pregnancies.sort_by(|left, right| {
        (left.conceived_minute, left.id.as_str()).cmp(&(right.conceived_minute, right.id.as_str()))
    });
    for mut pregnancy in pregnancies {
        pregnancy.parsed_state()?;
        pregnancy.status = PregnancyStatus::Ended;
        pregnancy.resolved_minute = Some(death_minute);
        ctx.db.pregnancy().id().update(pregnancy.clone());
        if ctx
            .db
            .active_pregnancy()
            .mother_id()
            .find(character_id)
            .is_some_and(|active| active.pregnancy_id == pregnancy.id)
        {
            ctx.db.active_pregnancy().mother_id().delete(character_id);
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
    Ok(())
}

/// Reserve two people now and schedule their marriage a year later.  The
/// scheduling transaction has no player-clock write and therefore remains a
/// canonical exclusive event even when ordinary social edges are asynchronous.
pub(crate) fn reserve_wedding(
    ctx: &ReducerContext,
    first_character_id: u64,
    second_character_id: u64,
    scheduled_from_minute: u64,
) -> Result<ExclusiveCommitment, CourtshipPairError> {
    if first_character_id == second_character_id {
        return Err("A character cannot marry themself".into());
    }
    let (first, second) = canonical_pair(first_character_id, second_character_id);
    let courtship_id = format!("courtship:{first}:{second}");
    if relationship_conflicts_at(ctx, first, scheduled_from_minute, Some(&courtship_id))
        || relationship_conflicts_at(ctx, second, scheduled_from_minute, Some(&courtship_id))
    {
        return Err(CourtshipPairError::rejected(
            CourtshipRejectionCode::ExclusiveCommitment,
            "A historical exclusive relationship conflicts at the wedding date",
        ));
    }
    for participant in [first, second] {
        if let Some(existing) = ctx
            .db
            .exclusive_commitment_participant()
            .character_id()
            .find(participant)
        {
            return Err(CourtshipPairError::rejected(
                CourtshipRejectionCode::ExclusiveCommitment,
                format!(
                    "Character already has exclusive commitment {}",
                    existing.commitment_id
                ),
            ));
        }
        if let Some(existing) = ctx
            .db
            .marriage_participant()
            .character_id()
            .find(participant)
        {
            return Err(CourtshipPairError::rejected(
                CourtshipRejectionCode::AlreadyMarried,
                format!(
                    "Character is already in active marriage {}",
                    existing.marriage_id
                ),
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
        .ok_or_else(|| {
            CourtshipPairError::rejected(
                CourtshipRejectionCode::CeremonySettlementRequired,
                "Wedding scheduling requires a shared ceremony settlement",
            )
        })?;
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
    let courtship = ctx.db.courtship().id().find(&courtship_id);
    let dowry_escrow = courtship.as_ref().and_then(|courtship| {
        (courtship.kind == CourtshipKind::Formal && courtship.planned_dowry_amount > 0)
            .then_some(courtship.approved_father_id)
            .flatten()
            .map(|father_id| (father_id, courtship.planned_dowry_amount))
    });
    if let Some((father_id, amount)) = dowry_escrow {
        if crate::item::personal_currency_total(ctx, father_id) < u64::from(amount) {
            return Err("The approved dowry is no longer available to reserve".into());
        }
        crate::item::validate_personal_currency_credit(ctx, &row.ceremony_settlement_id, amount)?;
        crate::item::consume_personal_currency(ctx, father_id, u64::from(amount))?;
    }
    ctx.db.exclusive_commitment().insert(row.clone());
    if let Some((father_id, amount)) = dowry_escrow {
        ctx.db.dowry_escrow().insert(DowryEscrow {
            commitment_id: id.clone(),
            father_id,
            amount,
            reserved_minute: scheduled_from_minute,
        });
    }
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
