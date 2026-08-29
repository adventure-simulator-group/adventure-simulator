// Owns stable lifecycle queue entries, processability, failure recording, and quarantine.
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

    fn processable(&self, ctx: &ReducerContext) -> bool {
        match self {
            Self::Wedding {
                effective_minute,
                id,
                ..
            } => ctx
                .db
                .exclusive_commitment()
                .id()
                .find(id)
                .is_some_and(|commitment| {
                    let participants = [
                        commitment.first_character_id,
                        commitment.second_character_id,
                    ];
                    let participant_died_before_ceremony =
                        participants.into_iter().any(|character_id| {
                            ctx.db
                                .character_death()
                                .character_id()
                                .find(character_id)
                                .is_some_and(|death| death.strategic_minute <= *effective_minute)
                        });
                    participant_died_before_ceremony
                        || participants.into_iter().all(|character_id| {
                            canonical_now(ctx, character_id)
                                .is_ok_and(|frontier| frontier >= *effective_minute)
                        })
                }),
            Self::Birth {
                effective_minute,
                mother_id,
                ..
            } => canonical_now(ctx, *mother_id).is_ok_and(|frontier| frontier >= *effective_minute),
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
    let id = format!(
        "lifecycle-failure:{}:{event_id}:{effective_minute}",
        event_kind.stable_id()
    );
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
    let Some(mut pregnancy) = ctx.db.pregnancy().id().find(pregnancy_id.to_owned()) else {
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
