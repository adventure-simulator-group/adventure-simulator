// Owns wedding settlement, dowry policy, and marriage terminal lifecycle.
pub fn settle_due_weddings(
    ctx: &ReducerContext,
    participant_id: u64,
    _participant_frontier: u64,
) -> Result<(), String> {
    let now = crate::time::refresh_clock(ctx)?;
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
        let participant_death_minute = [
            commitment.first_character_id,
            commitment.second_character_id,
        ]
        .into_iter()
        .filter_map(|character_id| {
            ctx.db
                .character_death()
                .character_id()
                .find(character_id)
                .map(|death| death.strategic_minute)
        })
        .filter(|death_minute| *death_minute <= effective_minute)
        .min();
        if let Some(death_minute) = participant_death_minute {
            transition_commitment_terminal(
                ctx,
                commitment,
                CommitmentStatus::Cancelled,
                CommitmentTerminalReason::ParticipantDead,
                death_minute,
            )?;
            continue;
        }
        // Ceremonies are canonical-world events. Subjective character clocks
        // neither delay the ceremony nor need to synchronize with each other.
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
            )?;
            continue;
        };
        let Some(second) = ctx.db.character().id().find(commitment.second_character_id) else {
            transition_commitment_terminal(
                ctx,
                commitment,
                CommitmentStatus::Cancelled,
                CommitmentTerminalReason::ParticipantDead,
                effective_minute,
            )?;
            continue;
        };
        if !character_alive_at(ctx, first.id, effective_minute)
            || !character_alive_at(ctx, second.id, effective_minute)
        {
            transition_commitment_terminal(
                ctx,
                commitment,
                CommitmentStatus::Cancelled,
                CommitmentTerminalReason::ParticipantDead,
                effective_minute,
            )?;
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
            )?;
            continue;
        }
        // Scheduling reserves attendance in the ceremony settlement. Resolve
        // housing from effective-dated legal history, never from a mutable
        // location or primary-residence pointer written after the ceremony.
        let mut residence_candidates: Vec<_> = ctx
            .db
            .residence_holding()
            .iter()
            .filter(|holding| {
                [first.id, second.id].contains(&holding.owner_character_id)
                    && holding.settlement_id == commitment.ceremony_settlement_id
                    && holding.acquired_minute <= effective_minute
                    && holding
                        .resolved_minute
                        .is_none_or(|resolved| resolved > effective_minute)
                    && crate::residence::holding_active_at(ctx, &holding.id, effective_minute)
            })
            .collect();
        residence_candidates.sort_by(|left, right| {
            (left.acquired_minute, left.id.as_str())
                .cmp(&(right.acquired_minute, right.id.as_str()))
        });
        let residence_holding_id = residence_candidates
            .into_iter()
            .next()
            .map(|holding| holding.id);
        let Some(residence_holding_id) = residence_holding_id else {
            transition_commitment_terminal(
                ctx,
                commitment,
                CommitmentStatus::Cancelled,
                CommitmentTerminalReason::ResidenceUnavailable,
                effective_minute,
            )?;
            continue;
        };
        let courtship_id = format!(
            "courtship:{}:{}",
            first.id.min(second.id),
            first.id.max(second.id)
        );
        let courtship = ctx.db.courtship().id().find(&courtship_id);
        let formal = courtship
            .as_ref()
            .is_some_and(|courtship| courtship.kind == CourtshipKind::Formal);
        let (_bride_id, recipient_id) = [first.id, second.id]
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
        } else if let Some(father) = courtship
            .as_ref()
            .and_then(|courtship| courtship.approved_father_id)
        {
            let amount = courtship
                .as_ref()
                .map_or(0, |courtship| courtship.planned_dowry_amount);
            if amount == 0 {
                (Some(father), 0, DowryOutcomeKind::NoDowry)
            } else if ctx
                .db
                .dowry_escrow()
                .commitment_id()
                .find(&commitment.id)
                .is_some_and(|escrow| escrow.father_id == father && escrow.amount == amount)
            {
                (Some(father), amount, DowryOutcomeKind::Paid)
            } else {
                (Some(father), amount, DowryOutcomeKind::InsufficientFunds)
            }
        } else {
            (None, 0, DowryOutcomeKind::FatherUnavailable)
        };
        // All fallible validation is complete before the first durable write.
        if let (Some(_father), amount, DowryOutcomeKind::Paid) = planned_dowry {
            crate::item::credit_personal_currency(
                ctx,
                recipient_id,
                &commitment.ceremony_settlement_id,
                amount,
            )?;
            ctx.db.dowry_escrow().commitment_id().delete(&commitment.id);
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
            crate::residence::move_residence_occupant_effective(
                ctx,
                &residence_holding_id,
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
        )?;
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
            .is_some_and(|member| {
                member.household_id == marriage.household_id && member.joined_minute <= minute
            })
        {
            leave_household(ctx, character_id);
        }
        crate::residence::remove_nonowned_occupancy_effective(ctx, character_id, minute);
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
    marriage.parsed_state()?;
    if actor_id != marriage.first_character_id && actor_id != marriage.second_character_id {
        return Err("Only a spouse can end this marriage".into());
    }
    let spouse_id = if actor_id == marriage.first_character_id {
        marriage.second_character_id
    } else {
        marriage.first_character_id
    };
    let actor_minute = enforce_temporal_scope(
        ctx,
        actor_id,
        Some(spouse_id),
        TemporalScope::ExclusiveShared,
    )?;
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
    let death_minute = [marriage.first_character_id, marriage.second_character_id]
        .into_iter()
        .filter_map(|id| {
            ctx.db
                .character_death()
                .character_id()
                .find(id)
                .map(|death| death.strategic_minute)
        })
        .filter(|death_minute| *death_minute <= minute)
        .min();
    let Some(death_minute) = death_minute else {
        return;
    };
    let both_reached_resolution = [marriage.first_character_id, marriage.second_character_id]
        .into_iter()
        .all(|id| canonical_now(ctx, id).is_ok_and(|frontier| frontier >= death_minute));
    if both_reached_resolution {
        resolve_marriage(ctx, marriage, MarriageStatus::Widowed, death_minute);
    }
}
