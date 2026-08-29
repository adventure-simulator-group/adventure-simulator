// Owns scheduled and immediate activity outcomes and their reducer boundary.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ActivityRisks {
    pub thievery_discovery: f32,
    pub raiding_retaliation: f32,
    pub carousing_disorder: f32,
}

fn apply_activity_outcomes(
    ctx: &ReducerContext,
    character_id: u64,
    schedule: &ScheduleAllocation,
    elapsed: u64,
    interval_end_minute: u64,
) -> Result<ActivityRisks, String> {
    apply_activity_outcomes_inner(
        ctx,
        character_id,
        schedule,
        elapsed,
        interval_end_minute,
        true,
    )
}

fn apply_activity_outcomes_without_leisure(
    ctx: &ReducerContext,
    character_id: u64,
    schedule: &ScheduleAllocation,
    elapsed: u64,
    interval_end_minute: u64,
) -> Result<ActivityRisks, String> {
    apply_activity_outcomes_inner(
        ctx,
        character_id,
        schedule,
        elapsed,
        interval_end_minute,
        false,
    )
}

fn apply_activity_outcomes_inner(
    ctx: &ReducerContext,
    character_id: u64,
    schedule: &ScheduleAllocation,
    elapsed: u64,
    interval_end_minute: u64,
    apply_leisure: bool,
) -> Result<ActivityRisks, String> {
    let location = activity_execution_location(ctx, character_id)?;
    let Some(settlement_id) = location.origin_settlement_id.as_ref() else {
        return Ok(ActivityRisks::default());
    };
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(settlement_id)
        .ok_or("Character's settlement not found")?;
    let attributes: CharacterAttributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .ok_or("Character attributes not found")?;
    let limbs = ctx
        .db
        .character_limbs()
        .character_id()
        .find(character_id)
        .ok_or("Character limbs not found")?;
    let skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Character skills not found")?;
    let stats: CharacterStats = ctx
        .db
        .character_stats()
        .character_id()
        .find(character_id)
        .ok_or("Character stats not found")?;
    let equipment = StrategicEquipment::load(ctx, character_id);
    let strength = attributes.limb_attr_by_weight_by_parts(
        LimbAttribute::Strength,
        &limbs,
        LimbWeights::all_equal(),
    );
    let endurance = attributes.attr_by_parts(SimpleAttribute::Endurance, &limbs);
    let stealth = skills.skill_check_by_parts(
        Skill::Stealth,
        &attributes,
        &limbs,
        &stats,
        &equipment,
        LimbWeights::all_equal(),
    );
    let capability = crate::capability::evaluate_character(ctx, character_id)?;
    let population = adventuresim_core::activity::settlement_population_scale(
        settlement.population_level,
        settlement.population_estimate,
    );
    let combat = capability
        .weapon_precision
        .max(capability.athletics)
        .max(capability.endurance);
    let outcome = settlement_activity_outcome(
        core_schedule(schedule),
        elapsed,
        ActivityOutcomeInputs {
            strength_check: strength,
            endurance_check: endurance,
            stealth_check: stealth,
            combat_check: combat,
            population_scale: population,
        },
    );
    if outcome.gold_earned > 0 {
        crate::item::credit_personal_currency(
            ctx,
            character_id,
            &settlement.id,
            outcome.gold_earned,
        )?;
    }
    if outcome.carousing_morale > 0.0 {
        crate::condition::record_morale_event(
            ctx,
            character_id,
            adventuresim_core::morale::MoraleEventKind::Carousing,
            outcome.carousing_morale,
            Some("activity:carousing".into()),
        )?;
    }
    apply_organization_outcomes(ctx, character_id, schedule, elapsed, &settlement.id)?;
    if outcome.infamy_gained > 0.0 {
        crate::reputation::record_event(
            ctx,
            format!("activity:{character_id}:{interval_end_minute}:crime"),
            character_id,
            &settlement.id,
            "criminal_activity",
            &interval_end_minute.to_string(),
            0,
            (outcome.infamy_gained * 100.0).round() as i32,
            interval_end_minute,
        )?;
    }
    if apply_leisure {
        crate::condition::apply_settlement_leisure_condition(
            ctx,
            character_id,
            core_schedule(schedule),
            elapsed,
            interval_end_minute,
        )?;
        crate::relationship::apply_spouse_leisure_conception(
            ctx,
            character_id,
            interval_end_minute.saturating_sub(elapsed),
            interval_end_minute,
            core_schedule(schedule),
        )?;
    }
    Ok(ActivityRisks {
        thievery_discovery: outcome.thievery_discovery_chance,
        raiding_retaliation: outcome.raiding_retaliation_chance,
        carousing_disorder: {
            let multiplier =
                match crate::personality::personality_or_neutral(ctx, character_id).temperance {
                    crate::personality::Temperance::Drunkard => 3.0,
                    crate::personality::Temperance::Temperate => 0.35,
                    crate::personality::Temperance::Neutral => 1.0,
                };
            (outcome.carousing_disorder_chance * multiplier).clamp(0.0, 0.95)
        },
    })
}

fn immediate_activity_schedule(
    activity: ImmediateActivity,
    minutes: u16,
    organization_id: Option<&str>,
) -> ScheduleAllocation {
    let mut schedule = ScheduleAllocation::default();
    match activity {
        ImmediateActivity::Prayer => schedule.prayer_minutes = minutes,
        ImmediateActivity::Reading => schedule.reading_minutes = minutes,
        ImmediateActivity::CombatTraining => schedule.combat_training_minutes = minutes,
        ImmediateActivity::Carousing => schedule.carousing_minutes = minutes,
        ImmediateActivity::Apprenticeship => {
            schedule.apprenticeship_minutes = minutes;
            schedule.apprenticeship_organization_id = organization_id.map(str::to_owned);
        }
        ImmediateActivity::ProfessionPractice => {
            schedule.profession_practice_minutes = minutes;
            schedule.practice_organization_id = organization_id.map(str::to_owned);
        }
        ImmediateActivity::Labor => schedule.labor_minutes = minutes,
        ImmediateActivity::Thievery => schedule.thievery_minutes = minutes,
        ImmediateActivity::Raiding => schedule.raiding_minutes = minutes,
    }
    schedule
}

/// Perform one selected activity continuously. Unlike settlement rest this
/// neither convalesces, repairs, washes, heals, supplies an inn, nor consults
/// or mutates the saved daily plan.
#[reducer]
pub fn perform_immediate_activity(
    ctx: &ReducerContext,
    character_id: u64,
    activity: ImmediateActivity,
    requested_minutes: u64,
    organization_id: Option<&str>,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    let character = crate::character::require_living_character(ctx, character_id)?;
    crate::strategic::require_character_no_unresolved_encounter(ctx, character_id)?;
    if let Some(party_id) = character.party_id.as_ref()
        && ctx
            .db
            .strategic_incident()
            .party_id()
            .filter(party_id)
            .any(|incident| incident.status == crate::strategic::IncidentStatus::Pending)
    {
        return Err("Resolve the strategic incident before performing an activity".into());
    }
    let location = activity_execution_location(ctx, character_id)?;
    if let Some(location_activity) = location_activity(activity)
        && let Some(reason) = location.policy.unavailable_reason(location_activity)
    {
        return Err(reason.into());
    }
    if location.policy == ActivityLocation::NamedOutdoorLocation
        && !matches!(activity, ImmediateActivity::Raiding)
    {
        return Err("This activity may only be performed at a settlement".into());
    }
    if location.policy == ActivityLocation::JourneyCamp {
        return Err(
            "Immediate activities are unavailable while travelling or at a journey camp".into(),
        );
    }
    if location.policy == ActivityLocation::IneligibleNamedLocation {
        return Err("Immediate activities are unavailable at this location".into());
    }
    if !(60..=MINUTES_PER_DAY).contains(&requested_minutes) || !requested_minutes.is_multiple_of(60)
    {
        return Err("Activity duration must use whole hours from one to 24 hours".into());
    }
    ensure_character_time(ctx, character_id)?;
    let _ = refresh_clock(ctx)?;
    let minutes = u16::try_from(requested_minutes).map_err(|_| "Activity duration is too long")?;
    let schedule = immediate_activity_schedule(activity, minutes, organization_id);
    validate_organization_schedule(ctx, character_id, &schedule)?;

    let mut character_time = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or("Character time record not found")?;
    let starting_minute = character_time.minutes;
    let injury_limit = crate::surgery::preview_injury_boundary(
        ctx,
        character_id,
        requested_minutes,
        InjuryRecoveryMinutes::NONE,
    )?
    .elapsed;
    let (elapsed, terminal) =
        crate::disease::clip_elapsed_for_disease(ctx, character_id, injury_limit, false)?;
    let settled =
        crate::surgery::settle_injuries(ctx, character_id, elapsed, InjuryRecoveryMinutes::NONE)?;
    let elapsed = settled.elapsed;
    character_time.minutes = character_time
        .minutes
        .checked_add(elapsed)
        .ok_or("Character clock overflow")?;
    let interval_end = character_time.minutes;
    ctx.db
        .character_time()
        .character_id()
        .update(character_time);
    let at_settlement = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .is_some_and(|character| character.current_settlement_id.is_some());
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
    crate::condition::apply_elapsed_needs(ctx, character_id, elapsed)?;
    crate::disease::finish_disease_interval(ctx, character_id, terminal)?;
    settle_lifecycle_after_character_time_write(ctx, character_id, interval_end)?;
    advance_married_family_by(ctx, character_id, elapsed)?;
    if terminal.is_some() || !settled.alive {
        crate::organization::settle_membership_dues(ctx, character_id)?;
        return Ok(());
    }

    // The activity allocation describes this one interval directly. Applying
    // it over one canonical day makes both linear and saturating effects use
    // the selected number of minutes, while the personal clock advances only
    // by the actual interval (which may have been clipped by an incident).
    let effective_minutes = u16::try_from(elapsed.min(requested_minutes)).unwrap_or(minutes);
    let effective_schedule =
        immediate_activity_schedule(activity, effective_minutes, organization_id);
    let mut skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Character skills not found")?;
    let profile = activity_training_profile(ctx, character_id)?;
    let excess = apply_training(
        ctx,
        character_id,
        &mut skills,
        &effective_schedule,
        MINUTES_PER_DAY,
        profile,
    )?;
    crate::condition::record_mastery_training_morale(
        ctx,
        character_id,
        u64::from(effective_minutes),
        excess,
    );
    ctx.db.character_skills().character_id().update(skills);
    let risks = apply_activity_outcomes_without_leisure(
        ctx,
        character_id,
        &effective_schedule,
        MINUTES_PER_DAY,
        interval_end,
    )?;
    if matches!(activity, ImmediateActivity::Prayer) {
        crate::condition::record_immediate_prayer_morale(ctx, character_id, effective_minutes)?;
    }
    if matches!(activity, ImmediateActivity::Labor) {
        let mut stats = ctx
            .db
            .character_stats()
            .character_id()
            .find(character_id)
            .ok_or("Character stats not found")?;
        stats.calories_used += f32::from(effective_minutes) / 60.0
            * adventuresim_core::strategic_schedule::LABOR_FATIGUE_PER_HOUR;
        ctx.db.character_stats().character_id().update(stats);
    }
    crate::strategic::maybe_trigger_activity_incident(ctx, character_id, risks)?;
    crate::capability::refresh_character_capability(ctx, character_id)?;
    crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    crate::organization::settle_membership_dues(ctx, character_id)?;
    Ok(())
}

const ACTIVITY_MINUTE_SCALE: u64 = MINUTES_PER_DAY;

fn apply_organization_outcomes(
    ctx: &ReducerContext,
    character_id: u64,
    schedule: &ScheduleAllocation,
    elapsed: u64,
    settlement_id: &str,
) -> Result<(), String> {
    if schedule.apprenticeship_minutes > 0 {
        let organization_id = schedule
            .apprenticeship_organization_id
            .as_deref()
            .ok_or("Apprenticeship time requires an organization")?;
        crate::organization::increment_activity_accrual(
            ctx,
            character_id,
            organization_id,
            elapsed.saturating_mul(u64::from(schedule.apprenticeship_minutes)),
            0,
        );
    }
    if schedule.profession_practice_minutes > 0 {
        let organization_id = schedule
            .practice_organization_id
            .as_deref()
            .ok_or("Professional practice time requires an organization")?;
        let mut row = crate::organization::membership(ctx, character_id, organization_id)
            .ok_or("Eligible organization membership disappeared during the interval")?;
        let definition = adventuresim_core::organization::organization(organization_id)
            .ok_or("Unknown organization")?;
        let role = crate::organization::membership_role(ctx, &row)?;
        let old = row.practice_minutes_accrued;
        row.practice_minutes_accrued = old.saturating_add(
            elapsed.saturating_mul(u64::from(schedule.profession_practice_minutes)),
        );
        let interval =
            u64::from(role.practice_reward_interval_minutes).saturating_mul(ACTIVITY_MINUTE_SCALE);
        if interval == 0 {
            return Err("Eligible organization role has no practice reward cadence".into());
        }
        let reward = row.practice_minutes_accrued / interval - old / interval;
        match definition.activity.reward {
            adventuresim_core::organization::ActivityReward::Gold if reward > 0 => {
                crate::item::credit_personal_currency(
                    ctx,
                    character_id,
                    settlement_id,
                    u32::try_from(reward).unwrap_or(u32::MAX),
                )?;
            }
            adventuresim_core::organization::ActivityReward::Fame if reward > 0 => {
                let minute = ctx
                    .db
                    .character_time()
                    .character_id()
                    .find(character_id)
                    .map_or(0, |time| time.minutes);
                crate::reputation::record_event(
                    ctx,
                    format!("profession:{character_id}:{organization_id}:{minute}"),
                    character_id,
                    settlement_id,
                    "religious_practice",
                    organization_id,
                    i32::try_from(reward.saturating_mul(100)).unwrap_or(i32::MAX),
                    0,
                    minute,
                )?;
            }
            _ => {}
        }
        ctx.db.organization_membership().id().update(row);
    }
    Ok(())
}
