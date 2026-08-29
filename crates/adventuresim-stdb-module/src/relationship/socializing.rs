// Owns scheduled socializing selection, chronology, affinity effects, and receipts.
fn socializing_id(actor_id: u64, day: u64, target_id: u64) -> String {
    format!("socializing:{actor_id}:{day}:{target_id}")
}

/// Project a directional affinity at an effective relationship minute.
///
/// A row whose anchor is newer than the requested minute cannot be
/// reconstructed from compact soft state, so callers fail closed instead of
/// letting a future opinion authorize a backdated exclusive relationship.
fn affinity_at(ctx: &ReducerContext, subject_id: u64, actor_id: u64, minute: u64) -> Option<f32> {
    let Some(row) = ctx
        .db
        .character_affinity()
        .id()
        .find(format!("{subject_id}:{actor_id}"))
    else {
        return Some(0.0);
    };
    (row.anchor_minute <= minute).then(|| {
        adventuresim_core::social::settle_affinity(
            row.anchor,
            minute.saturating_sub(row.anchor_minute),
        )
    })
}

fn active_romantic_partners(
    ctx: &ReducerContext,
    actor_id: u64,
    effective_minute: u64,
) -> Vec<u64> {
    ctx.db
        .courtship()
        .iter()
        .filter(|row| {
            row.started_minute <= effective_minute
                && row
                    .resolved_minute
                    .is_none_or(|resolved| resolved > effective_minute)
                && (row.first_character_id == actor_id || row.second_character_id == actor_id)
        })
        .map(|courtship| {
            if courtship.first_character_id == actor_id {
                courtship.second_character_id
            } else {
                courtship.first_character_id
            }
        })
        .collect()
}

fn socializing_target(
    ctx: &ReducerContext,
    actor_id: u64,
    day: u64,
    effective_minute: u64,
) -> Option<u64> {
    let actor = ctx.db.character().id().find(actor_id)?;
    let same_settlement = |candidate: &crate::Character| {
        if !character_alive_at(ctx, candidate.id, effective_minute) || candidate.id == actor_id {
            return false;
        }
        if let Some(presence) = ctx
            .db
            .settlement_resident_presence()
            .character_id()
            .find(candidate.id)
        {
            return actor.current_settlement_id.as_deref() == Some(&presence.settlement_id)
                && npc_is_present(ctx, &presence, effective_minute);
        }
        // A mutable location without an interval history is authoritative
        // only at the character's own frontier. Fail closed for historical
        // selection rather than leaking a future move into this slice.
        (canonical_now(ctx, candidate.id)
            .is_ok_and(|candidate_minute| candidate_minute <= effective_minute)
            || ctx
                .db
                .character_death()
                .character_id()
                .find(candidate.id)
                .is_some_and(|death| death.strategic_minute > effective_minute))
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
    let available_partners = active_romantic_partners(ctx, actor_id, effective_minute)
        .into_iter()
        .filter(|partner| {
            ctx.db
                .character()
                .id()
                .find(*partner)
                .is_some_and(|candidate| same_settlement(&candidate))
        })
        .collect();
    if let Some(partner) = choose(available_partners) {
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

/// Earliest known boundary at which the deterministic socializing priority
/// list can change. Resident schedules are recurring location histories; hard
/// birth/death and courtship timestamps are durable one-time histories.
fn next_socializing_boundary(
    ctx: &ReducerContext,
    actor_id: u64,
    start_minute: u64,
    end_minute: u64,
) -> Option<u64> {
    let actor_settlement = ctx
        .db
        .character()
        .id()
        .find(actor_id)
        .and_then(|actor| actor.current_settlement_id);
    let day_start = (start_minute / MINUTES_PER_DAY).saturating_mul(MINUTES_PER_DAY);
    let resident: Vec<u64> = actor_settlement.map_or_else(Vec::new, |settlement_id| {
        ctx.db
            .settlement_resident_presence()
            .iter()
            .filter(|presence| presence.settlement_id == settlement_id)
            .flat_map(|presence| {
                [presence.start_minute, presence.end_minute]
                    .into_iter()
                    .map(|offset| day_start.saturating_add(u64::from(offset)))
            })
            .collect()
    });
    let births = ctx
        .db
        .character_birth()
        .iter()
        .filter_map(|birth| u64::try_from(birth.birth_minute).ok());
    let deaths = ctx
        .db
        .character_death()
        .iter()
        .map(|death| death.strategic_minute);
    let courtships = ctx
        .db
        .courtship()
        .iter()
        .filter(move |courtship| {
            courtship.first_character_id == actor_id || courtship.second_character_id == actor_id
        })
        .flat_map(|courtship| [Some(courtship.started_minute), courtship.resolved_minute])
        .flatten();
    resident
        .into_iter()
        .chain(births)
        .chain(deaths)
        .chain(courtships)
        .filter(|minute| start_minute < *minute && *minute < end_minute)
        .min()
}

fn record_socializing_receipt(
    ctx: &ReducerContext,
    actor_id: u64,
    target_id: u64,
    day: u64,
    start_minute: u64,
    end_minute: u64,
    minutes: u64,
) {
    let id = socializing_id(actor_id, day, target_id);
    let existing = ctx.db.socializing_receipt().id().find(&id);
    let receipt = SocializingReceipt {
        id,
        actor_id,
        target_id,
        day,
        start_minute: existing.as_ref().map_or(start_minute, |receipt| {
            receipt.start_minute.min(start_minute)
        }),
        end_minute,
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
        let applied_through = ctx
            .db
            .socializing_receipt()
            .actor_id()
            .filter(actor_id)
            .filter(|receipt| receipt.day == day)
            .map(|receipt| receipt.end_minute)
            .max()
            .unwrap_or(start)
            .max(start);
        let mut cursor = applied_through.min(end);
        while cursor < end {
            let slice_end = next_socializing_boundary(ctx, actor_id, cursor, end).unwrap_or(end);
            let minutes = allocation(slice_end).saturating_sub(allocation(cursor));
            // Select against the beginning of each availability slice. The
            // actor's stored clock already points at `interval_end`, so a
            // future death or recurring resident departure must not rewrite
            // the earlier part of a bulk advance.
            let Some(target_id) = socializing_target(ctx, actor_id, day, cursor) else {
                // The actor id is an impossible real target and therefore a
                // private zero-minute watermark. It prevents a later chunk
                // from retroactively realizing time for which nobody was
                // available.
                record_socializing_receipt(ctx, actor_id, actor_id, day, cursor, slice_end, 0);
                cursor = slice_end;
                continue;
            };
            if minutes > 0 {
                let _ = enforce_temporal_scope(
                    ctx,
                    actor_id,
                    Some(target_id),
                    TemporalScope::PairwiseSoft,
                )?;
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
            }
            record_socializing_receipt(ctx, actor_id, target_id, day, cursor, slice_end, minutes);
            cursor = slice_end;
        }
    }
    Ok(())
}
