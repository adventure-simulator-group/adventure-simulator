// Owns official and personal clock initialization, refresh, and family-clock propagation.
pub fn initialize_time(ctx: &ReducerContext) {
    if ctx.db.world_clock().id().find(0).is_none() {
        ctx.db.world_clock().insert(WorldClock {
            id: 0,
            official_minutes: WORLD_START_MINUTE,
            epoch_micros: ctx.timestamp.to_micros_since_unix_epoch(),
        });
    }
}

pub fn refresh_clock(ctx: &ReducerContext) -> Result<u64, String> {
    if ctx.db.world_clock().id().find(0).is_none() {
        initialize_time(ctx);
    }
    let mut clock = ctx
        .db
        .world_clock()
        .id()
        .find(0)
        .ok_or_else(|| "World clock is not initialized".to_string())?;
    let official_minutes = calculate_official_minutes(
        clock.epoch_micros,
        ctx.timestamp.to_micros_since_unix_epoch(),
    );
    if official_minutes != clock.official_minutes {
        clock.official_minutes = official_minutes;
        ctx.db.world_clock().id().update(clock);
    }
    Ok(official_minutes)
}

fn married_family_npc_ids(ctx: &ReducerContext, character_id: u64) -> Vec<u64> {
    let character_party_id = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .and_then(|character| character.party_id);
    let active_marriage = ctx.db.marriage().iter().find(|row| {
        row.status == MarriageStatus::Active
            && (row.first_character_id == character_id || row.second_character_id == character_id)
    });
    let Some(marriage) = active_marriage else {
        return Vec::new();
    };
    let mut related = std::collections::BTreeSet::from([
        marriage.first_character_id,
        marriage.second_character_id,
    ]);
    related.extend(
        ctx.db
            .household_member()
            .household_id()
            .filter(&marriage.household_id)
            .map(|row| row.character_id),
    );
    related.extend(
        ctx.db
            .character_kinship()
            .subject_id()
            .filter(character_id)
            .map(|row| row.related_id),
    );
    related.remove(&character_id);
    related
        .into_iter()
        .filter(|related_id| {
            ctx.db
                .npc_policy()
                .character_id()
                .find(*related_id)
                .is_some()
        })
        .filter(|related_id| {
            ctx.db
                .character()
                .id()
                .find(*related_id)
                .is_none_or(|related| related.party_id != character_party_id)
        })
        .take(32)
        .collect()
}

fn advance_married_family_by(
    ctx: &ReducerContext,
    character_id: u64,
    elapsed: u64,
) -> Result<(), String> {
    if elapsed == 0
        || ctx
            .db
            .npc_policy()
            .character_id()
            .find(character_id)
            .is_some()
    {
        return Ok(());
    }
    for related_id in married_family_npc_ids(ctx, character_id) {
        if !ctx
            .db
            .character()
            .id()
            .find(related_id)
            .is_some_and(|character| character.alive)
        {
            continue;
        }
        ensure_character_time(ctx, related_id)?;
        let target = ctx
            .db
            .character_time()
            .character_id()
            .find(related_id)
            .ok_or("Married family member has no subjective clock")?
            .minutes
            .saturating_add(elapsed);
        advance_stationary_character_to(ctx, related_id, target)?;
    }
    Ok(())
}

pub fn initialize_character_time(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    ensure_character_time(ctx, character_id)
}
