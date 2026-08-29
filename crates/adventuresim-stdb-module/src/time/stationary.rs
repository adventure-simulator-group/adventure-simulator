// Owns stationary catch-up and saved training-schedule mutation.
/// Advance one stationary character through their ordinary saved schedule to
/// an explicit personal frontier. This is shared by lazy player catch-up and
/// bounded autonomous NPC policy; callers choose the target, never the
/// character being observed.
pub(crate) fn advance_stationary_character_to(
    ctx: &ReducerContext,
    character_id: u64,
    target_minutes: u64,
) -> Result<(), String> {
    ensure_character_time(ctx, character_id)?;
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if !character.alive {
        // A corpse's strategic minute remains the death minute. Lazy reads must
        // not train, recover, consume provisions, or advance it.
        return Ok(());
    }
    let mut character_time = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character time record not found".to_string())?;
    if target_minutes < character_time.minutes {
        return Err("Character time cannot be advanced retroactively".into());
    }
    let requested_elapsed = target_minutes.saturating_sub(character_time.minutes);
    if requested_elapsed == 0 {
        return Ok(());
    }
    if let Some(boundary) = crate::relationship::next_lifecycle_boundary(
        ctx,
        character_id,
        character_time.minutes,
        target_minutes,
    ) {
        let was_npc_controlled = ctx
            .db
            .npc_policy()
            .character_id()
            .find(character_id)
            .is_some();
        advance_stationary_character_to(ctx, character_id, boundary)?;
        let alive = ctx
            .db
            .character()
            .id()
            .find(character_id)
            .is_some_and(|row| row.alive);
        let npc_authority_transferred = was_npc_controlled
            && ctx
                .db
                .npc_policy()
                .character_id()
                .find(character_id)
                .is_none();
        if !alive || npc_authority_transferred {
            return Ok(());
        }
        return advance_stationary_character_to(ctx, character_id, target_minutes);
    }
    let starting_minute = character_time.minutes;
    let saved_schedule = ctx
        .db
        .character_training_schedule()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character training schedule not found".to_string())?;
    let execution_location = activity_execution_location(ctx, character_id)?;
    let organization_schedule =
        effective_organization_schedule(ctx, character_id, &saved_schedule.downtime);
    let effective_schedule = if execution_location.policy == ActivityLocation::JourneyCamp {
        let camp_schedule = allowed_camp_schedule(&organization_schedule);
        effective_location_schedule(&camp_schedule, execution_location.policy, character_id)
    } else {
        effective_location_schedule(
            &organization_schedule,
            execution_location.policy,
            character_id,
        )
    };
    let injury_limit = crate::surgery::preview_injury_boundary(
        ctx,
        character_id,
        requested_elapsed,
        InjuryRecoveryMinutes::new(requested_elapsed),
    )?
    .elapsed;
    let (elapsed, terminal) =
        crate::disease::clip_elapsed_for_disease(ctx, character_id, injury_limit, true)?;
    let convalescing = convalescence_minutes(
        ctx,
        character_id,
        party_physiology_check(ctx, character_id)?,
    )
    .min(elapsed);
    let settled = crate::surgery::settle_injuries(
        ctx,
        character_id,
        elapsed,
        InjuryRecoveryMinutes::new(elapsed),
    )?;
    let elapsed = settled.elapsed;
    character_time.minutes = character_time.minutes.saturating_add(elapsed);
    ctx.db
        .character_time()
        .character_id()
        .update(character_time);
    advance_married_family_by(ctx, character_id, elapsed)?;
    let at_settlement = character.current_settlement_id.is_some();
    crate::condition::apply_weather_exposure(
        ctx,
        character_id,
        starting_minute,
        elapsed,
        false,
        if at_settlement {
            ExposureShelter::Indoor
        } else {
            ExposureShelter::Field(FieldShelter::Bivouac)
        },
    )?;
    crate::social::settle_shared_party_time(ctx, character_id);
    crate::disease::finish_disease_interval(ctx, character_id, terminal)?;
    settle_lifecycle_after_character_time_write(
        ctx,
        character_id,
        starting_minute.saturating_add(elapsed),
    )?;
    if terminal.is_some() || !settled.alive {
        crate::organization::settle_membership_dues(ctx, character_id)?;
        return Ok(());
    }
    let training_elapsed = elapsed.saturating_sub(convalescing);
    let socializing_before = total_socializing_receipt_minutes(ctx, character_id);
    if at_settlement && training_elapsed > 0 {
        crate::relationship::apply_scheduled_socializing(
            ctx,
            character_id,
            effective_schedule.socializing_minutes,
            target_minutes.saturating_sub(training_elapsed),
            target_minutes,
        )?;
    }
    let realized_socializing_minutes =
        total_socializing_receipt_minutes(ctx, character_id).saturating_sub(socializing_before);
    let mut realized_training_schedule = effective_schedule.clone();
    if realized_socializing_minutes == 0 {
        // An unavailable Socializing allocation is downtime/Leisure, not
        // imaginary practice of Charm, Insight, or Deception.
        realized_training_schedule.socializing_minutes = 0;
    }
    let mut skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character skill record not found".to_string())?;
    let activities = activity_training_profile(ctx, character_id)?;
    let excess = apply_training(
        ctx,
        character_id,
        &mut skills,
        &realized_training_schedule,
        training_elapsed,
        activities,
    )?;
    crate::condition::record_mastery_training_morale(ctx, character_id, training_elapsed, excess);
    ctx.db.character_skills().character_id().update(skills);
    let risks = apply_activity_outcomes(
        ctx,
        character_id,
        &effective_schedule,
        training_elapsed,
        target_minutes,
    )?;
    crate::strategic::maybe_trigger_activity_incident(ctx, character_id, risks)?;
    if at_settlement && training_elapsed > 0 {
        crate::social::apply_automatic_social_chats(ctx, character_id, training_elapsed)?;
    }
    if at_settlement {
        crate::condition::replenish_needs_at_settlement(ctx, character_id)?;
        crate::capability::refresh_character_capability(ctx, character_id)?;
    }
    crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    crate::organization::settle_membership_dues(ctx, character_id)?;
    Ok(())
}

#[reducer]
pub fn update_training_schedule(
    ctx: &ReducerContext,
    character_id: u64,
    downtime: ScheduleAllocation,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    ensure_character_time(ctx, character_id)?;
    let plan = adventuresim_core::strategic_schedule::validate_daily_allocation(
        [
            downtime.labor_minutes,
            downtime.prayer_minutes,
            downtime.thievery_minutes,
            downtime.raiding_minutes,
            downtime.combat_training_minutes,
            downtime.carousing_minutes,
            downtime.socializing_minutes,
            downtime.apprenticeship_minutes,
            downtime.profession_practice_minutes,
            downtime.reading_minutes,
        ],
        (
            downtime.apprenticeship_minutes,
            downtime.apprenticeship_organization_id.clone(),
        ),
        (
            downtime.profession_practice_minutes,
            downtime.practice_organization_id.clone(),
        ),
    )
    .map_err(|error| error.to_string())?;
    let [
        labor,
        prayer,
        thievery,
        raiding,
        combat_training,
        carousing,
        socializing,
        _,
        _,
        reading,
    ] = plan.activities;
    let downtime = ScheduleAllocation {
        reading_minutes: reading.get(),
        combat_training_minutes: combat_training.get(),
        carousing_minutes: carousing.get(),
        socializing_minutes: socializing.get(),
        apprenticeship_minutes: plan.apprenticeship.minutes(),
        apprenticeship_organization_id: plan.apprenticeship.organization_id(),
        profession_practice_minutes: plan.practice.minutes(),
        practice_organization_id: plan.practice.organization_id(),
        labor_minutes: labor.get(),
        prayer_minutes: prayer.get(),
        thievery_minutes: thievery.get(),
        raiding_minutes: raiding.get(),
    };
    let schedule = CharacterTrainingSchedule {
        character_id,
        downtime,
    };
    validate_organization_schedule(ctx, character_id, &schedule.downtime)?;
    ctx.db
        .character_training_schedule()
        .character_id()
        .update(schedule);
    crate::condition::refresh_character_strategic_condition(ctx, character_id).map(|_| ())
}
