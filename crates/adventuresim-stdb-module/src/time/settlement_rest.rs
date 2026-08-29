// Owns settlement rest authorization, payment, recovery, and arrival synchronization.
pub(crate) fn health_recovered_per_day(physiology_check: f32) -> f32 {
    BASE_HEALTH_RECOVERED_PER_DAY
        + physiology_check.clamp(0.0, 5.0) * HEALTH_RECOVERED_PER_PHYSIOLOGY_CHECK_PER_DAY
}

pub(crate) fn party_physiology_check(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<f32, String> {
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or_else(|| "Character not found".to_string())?;
    let member_ids: Vec<u64> = if let Some(party_id) = character.party_id {
        crate::strategic::living_party_member_ids(ctx, &party_id)
    } else {
        vec![character_id]
    };
    let checks = member_ids
        .into_iter()
        .map(|member_id| {
            crate::capability::evaluate_character(ctx, member_id)
                .map(|capabilities| capabilities.physiology)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(aggregate_bounded_party_check(checks))
}

fn convalescence_minutes(ctx: &ReducerContext, character_id: u64, physiology_check: f32) -> u64 {
    crate::surgery::convalescence_minutes(ctx, character_id, physiology_check)
}

/// Spend completed game days at a settlement. Injuries receive all selected
/// rest first; only the remaining time is eligible for scheduled training.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettlementRestProvision {
    PublicService(SettlementActionService),
    Residence,
    PrivateDowntime,
}

/// Spend an exact number of settlement minutes. This intentionally keeps
/// each character's clock independent: sharing a settlement does not force a
/// party to keep identical strategic times.
#[reducer]
pub fn rest_at_settlement_hours(
    ctx: &ReducerContext,
    character_id: u64,
    requested_minutes: u64,
    service: SettlementActionService,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    require_character_rest_service(ctx, character_id, service)?;
    rest_for_minutes(
        ctx,
        character_id,
        requested_minutes,
        SettlementRestProvision::PublicService(service),
        true,
        true,
        None,
    )
    .map(|_| ())
}
/// Rest at an active primary residence in the character's current settlement.
/// A residence supplies the same full board as an inn, but its recurring costs
/// are settled through the residence ledger rather than a per-stay fee.
#[reducer]
pub fn rest_at_residence_hours(
    ctx: &ReducerContext,
    character_id: u64,
    requested_minutes: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    require_character_residence_rest(ctx, character_id)?;
    rest_for_minutes(
        ctx,
        character_id,
        requested_minutes,
        SettlementRestProvision::Residence,
        true,
        true,
        None,
    )
    .map(|_| ())
}

fn patient_publicly_needs_rest(ctx: &ReducerContext, patient_id: u64) -> Result<bool, String> {
    let condition = ctx
        .db
        .character_strategic_condition()
        .character_id()
        .find(patient_id)
        .ok_or("Patient's public strategic condition is unavailable")?;
    let publicly_ill = ctx
        .db
        .character_illness_status()
        .character_id()
        .find(patient_id)
        .is_some_and(|illness| illness.symptomatic || illness.critical);
    Ok(condition.status != adventuresim_core::morale::IncapacitationStatus::Ready || publicly_ill)
}

/// Pay an inn directly for one day of a co-located party member's medically
/// necessary convalescence. The payer authorizes only the exact public quote;
/// no currency is transferred to the patient.
#[reducer]
pub fn sponsor_party_member_inn_rest(
    ctx: &ReducerContext,
    payer_id: u64,
    patient_id: u64,
    settlement_id: String,
    expected_cost: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, payer_id)?;
    if payer_id == patient_id {
        return Err("A patient who can pay must use ordinary settlement rest".into());
    }
    let payer = crate::character::require_living_character(ctx, payer_id)?;
    let patient = crate::character::require_living_character(ctx, patient_id)?;
    let party_id = payer
        .party_id
        .as_deref()
        .filter(|party_id| patient.party_id.as_deref() == Some(*party_id))
        .ok_or("Payer and patient must belong to the same party")?;
    for character_id in [payer_id, patient_id] {
        if !ctx
            .db
            .party_member()
            .party_id()
            .filter(party_id)
            .any(|member| member.character_id == character_id)
        {
            return Err("Payer and patient must have current party membership".into());
        }
    }
    if payer.current_settlement_id.as_deref() != Some(&settlement_id)
        || patient.current_settlement_id.as_deref() != Some(&settlement_id)
    {
        return Err("Payer and patient must be together at the named settlement".into());
    }
    require_character_rest_service(ctx, patient_id, SettlementActionService::Inn)?;
    if !patient_publicly_needs_rest(ctx, patient_id)? {
        return Err("Sponsored inn rest requires a patient who publicly needs recovery".into());
    }
    let authoritative_cost = inn_stay_cost(MINUTES_PER_DAY)?;
    if expected_cost != authoritative_cost {
        return Err("Sponsored inn quote is stale or invalid".into());
    }
    let patient_funds = crate::item::personal_currency_total(ctx, patient_id);
    if patient_funds >= authoritative_cost {
        return Err("Patient can afford ordinary inn rest without sponsorship".into());
    }
    let sponsorship_gap = authoritative_cost.saturating_sub(patient_funds);
    if crate::item::personal_currency_total(ctx, payer_id) < sponsorship_gap {
        return Err("Payer cannot afford the authoritative inn gap".into());
    }
    rest_for_minutes(
        ctx,
        patient_id,
        MINUTES_PER_DAY,
        SettlementRestProvision::PublicService(SettlementActionService::Inn),
        true,
        true,
        Some(payer_id),
    )
    .map(|_| ())
}

fn require_settlement_rest_service(
    profile: &adventuresim_world_schema::SettlementEconomyProfile,
    service: SettlementActionService,
) -> Result<(), String> {
    use adventuresim_core::settlement_economy::action_service_available;

    if action_service_available(profile, service) {
        Ok(())
    } else {
        Err("This settlement does not offer the requested rest service".into())
    }
}

fn require_character_rest_service(
    ctx: &ReducerContext,
    character_id: u64,
    service: SettlementActionService,
) -> Result<(), String> {
    let character = crate::character::require_living_character(ctx, character_id)?;
    let settlement_id = character
        .current_settlement_id
        .as_deref()
        .ok_or("Settlement rest requires the character to be at a settlement")?;
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(settlement_id.to_owned())
        .ok_or("Character's settlement not found")?;
    require_settlement_rest_service(&settlement.economy, service)
}

fn require_character_residence_rest(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    let character = crate::character::require_living_character(ctx, character_id)?;
    let settlement_id = character
        .current_settlement_id
        .as_deref()
        .ok_or("Settlement rest requires the character to be at a settlement")?;
    let (residence, presence) =
        crate::residence::active_residence_presence(ctx, character_id, settlement_id)
            .ok_or("You do not have a residence")?;
    debug_assert!(
        residence.status == crate::residence::ResidenceHoldingStatus::Active
            && residence.settlement_id == settlement_id
            && matches!(
                presence.place(),
                adventuresim_core::strategic_place::StrategicPlaceId::Residence { .. }
            )
    );
    Ok(())
}

fn rest_for_minutes(
    ctx: &ReducerContext,
    character_id: u64,
    requested_minutes: u64,
    provision: SettlementRestProvision,
    explicit: bool,
    automatic_social: bool,
    inn_sponsor_id: Option<u64>,
) -> Result<u64, String> {
    let character = crate::character::require_living_character(ctx, character_id)?;
    if character.current_settlement_id.is_none() {
        return Err("Settlement downtime requires the character to be at a settlement".into());
    }
    ensure_character_time(ctx, character_id)?;
    let _ = refresh_clock(ctx)?;
    let mut character_time = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character time record not found".to_string())?;
    if requested_minutes == 0 {
        return Ok(0);
    }
    let saved_schedule = ctx
        .db
        .character_training_schedule()
        .character_id()
        .find(character_id)
        .ok_or_else(|| "Character training schedule not found".to_string())?;
    let effective_schedule = effective_location_schedule(
        &effective_organization_schedule(ctx, character_id, &saved_schedule.downtime),
        activity_execution_location(ctx, character_id)?.policy,
        character_id,
    );
    let conversation_choice = character.party_id.as_ref().and_then(|party_id| {
        let snapshot: Vec<_> = crate::strategic::living_party_member_ids(ctx, party_id)
            .into_iter()
            .filter_map(|id| {
                ctx.db
                    .character_skills()
                    .character_id()
                    .find(id)
                    .map(|skills| {
                        let cap = ctx
                            .db
                            .character_attributes()
                            .character_id()
                            .find(id)
                            .map_or(0.0, |attributes| attributes.instinct * 1_000.0);
                        (id, skills.oral_languages, cap)
                    })
            })
            .collect();
        adventuresim_world_schema::party_common_oral_choices_capped(&snapshot)
            .into_iter()
            .find(|choice| choice.0 == character_id)
    });

    if explicit {
        validate_settlement_rest_minutes(requested_minutes)?;
    }

    let requested_cost = inn_stay_cost(requested_minutes)?;
    if matches!(
        provision,
        SettlementRestProvision::PublicService(SettlementActionService::Inn)
    ) {
        let patient_funds = crate::item::personal_currency_total(ctx, character_id);
        let sponsor_gap = requested_cost.saturating_sub(patient_funds);
        let payment_available = if sponsor_gap == 0 {
            true
        } else {
            inn_sponsor_id.is_some_and(|sponsor_id| {
                sponsor_id != character_id
                    && crate::item::personal_currency_total(ctx, sponsor_id) >= sponsor_gap
            })
        };
        if !payment_available {
            return Err("Not enough coin to pay for the inn stay".into());
        }
    }

    if explicit {
        crate::filth::wash_before_explicit_rest(ctx, character_id)?;
    }

    let starting_minute = character_time.minutes;
    let requested_recovery = adventuresim_core::strategic_schedule::restorative_leisure_minutes(
        core_schedule(&effective_schedule),
        starting_minute,
        requested_minutes,
    );
    let injury_limit = crate::surgery::preview_injury_boundary(
        ctx,
        character_id,
        requested_minutes,
        InjuryRecoveryMinutes::new(requested_recovery),
    )?
    .elapsed;
    let (elapsed, terminal) =
        crate::disease::clip_elapsed_for_disease(ctx, character_id, injury_limit, true)?;
    let physiology_check = party_physiology_check(ctx, character_id)?;
    let recovery_elapsed = adventuresim_core::strategic_schedule::restorative_leisure_minutes(
        core_schedule(&effective_schedule),
        starting_minute,
        elapsed,
    );
    let convalescing =
        convalescence_minutes(ctx, character_id, physiology_check).min(recovery_elapsed);
    let settled = crate::surgery::settle_injuries(
        ctx,
        character_id,
        elapsed,
        InjuryRecoveryMinutes::new(recovery_elapsed),
    )?;
    let elapsed = settled.elapsed;
    character_time.minutes = character_time
        .minutes
        .checked_add(elapsed)
        .ok_or("Character clock overflow")?;
    ctx.db
        .character_time()
        .character_id()
        .update(character_time);
    crate::condition::apply_weather_exposure(
        ctx,
        character_id,
        starting_minute,
        elapsed,
        false,
        ExposureShelter::Indoor,
    )?;
    if matches!(
        provision,
        SettlementRestProvision::PublicService(SettlementActionService::Inn)
    ) {
        let elapsed_cost = inn_stay_cost(elapsed)?;
        let patient_contribution =
            crate::item::personal_currency_total(ctx, character_id).min(elapsed_cost);
        let sponsor_contribution = elapsed_cost.saturating_sub(patient_contribution);
        crate::item::consume_personal_currency(ctx, character_id, patient_contribution)
            .map_err(|_| "Not enough coin to pay for the inn stay".to_string())?;
        if sponsor_contribution > 0 {
            let sponsor_id = inn_sponsor_id.ok_or("Inn sponsorship became unavailable")?;
            crate::item::consume_personal_currency(ctx, sponsor_id, sponsor_contribution)
                .map_err(|_| "Not enough coin to pay for the inn stay".to_string())?;
        }
    }
    crate::social::settle_shared_party_time(ctx, character_id);
    crate::condition::apply_settlement_rest_elapsed_needs(ctx, character_id, elapsed, provision)?;
    crate::condition::apply_settlement_leisure_condition(
        ctx,
        character_id,
        core_schedule(&effective_schedule),
        elapsed,
        starting_minute.saturating_add(elapsed),
    )?;
    crate::disease::finish_disease_interval(ctx, character_id, terminal)?;
    settle_lifecycle_after_character_time_write(
        ctx,
        character_id,
        starting_minute.saturating_add(elapsed),
    )?;
    advance_married_family_by(ctx, character_id, elapsed)?;
    if terminal.is_some() || !settled.alive {
        crate::organization::settle_membership_dues(ctx, character_id)?;
        return Ok(0);
    }
    crate::alcohol::process_rest_evenings(
        ctx,
        character_id,
        starting_minute,
        starting_minute.saturating_add(elapsed),
        true,
    )?;

    let (smithing_skill, tailoring_skill) = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .and_then(|skills| {
            let attributes = ctx
                .db
                .character_attributes()
                .character_id()
                .find(character_id)?;
            Some((
                Skill::Smithing
                    .capped_training_rank(skills.smithing_hours, &attributes)
                    .floor() as u8,
                Skill::Tailoring
                    .capped_training_rank(skills.tailoring_hours, &attributes)
                    .floor() as u8,
            ))
        })
        .unwrap_or((0, 0));
    let _maintenance_elapsed = crate::repair::field_repair(
        ctx,
        character_id,
        smithing_skill,
        tailoring_skill,
        recovery_elapsed.saturating_sub(convalescing),
    );
    let training_elapsed = elapsed;
    if training_elapsed > 0 {
        let mut skills = ctx
            .db
            .character_skills()
            .character_id()
            .find(character_id)
            .ok_or_else(|| "Character skill record not found".to_string())?;
        let activities = activity_training_profile(ctx, character_id)?;
        let mut excess = apply_training(
            ctx,
            character_id,
            &mut skills,
            &effective_schedule,
            training_elapsed,
            activities,
        )?;
        if let Some((_, language, coefficient)) = conversation_choice {
            excess += apply_oral_language_training(
                ctx,
                character_id,
                &mut skills.oral_languages,
                language,
                training_elapsed as f32 / 60.0 * (2.0 / 3.0) * coefficient,
            );
        }
        crate::condition::record_mastery_training_morale(
            ctx,
            character_id,
            training_elapsed,
            excess,
        );
        ctx.db.character_skills().character_id().update(skills);
        let risks = apply_activity_outcomes(
            ctx,
            character_id,
            &effective_schedule,
            training_elapsed,
            starting_minute.saturating_add(elapsed),
        )?;
        crate::strategic::maybe_trigger_activity_incident(ctx, character_id, risks)?;
    }

    crate::condition::apply_rest_condition(ctx, character_id, elapsed)?;
    crate::food::clear_stomach_fullness(ctx, character_id);
    crate::capability::refresh_character_capability(ctx, character_id)?;
    if automatic_social && recovery_elapsed > 0 {
        crate::social::apply_automatic_social_chats(ctx, character_id, recovery_elapsed)?;
    }
    crate::organization::settle_membership_dues(ctx, character_id)?;
    Ok(training_elapsed)
}

fn inn_stay_cost(requested_minutes: u64) -> Result<u64, String> {
    adventuresim_core::strategic_economy::inn_full_board_cost(requested_minutes)
        .ok_or_else(|| "Inn cost overflow".to_string())
}

fn validate_settlement_rest_minutes(requested_minutes: u64) -> Result<(), String> {
    if (MIN_SETTLEMENT_REST_MINUTES..=MAX_SETTLEMENT_REST_MINUTES).contains(&requested_minutes) {
        Ok(())
    } else {
        Err("Settlement rest must last between one hour and one year".into())
    }
}

/// Venue-neutral private downtime for system-owned clock synchronization,
/// convalescence, and private holy-day observance. Public service reducers
/// must authorize an Inn or Temple before entering `rest_for_minutes`.
pub(crate) fn spend_private_settlement_downtime(
    ctx: &ReducerContext,
    character_id: u64,
    requested_minutes: u64,
    explicit: bool,
) -> Result<(), String> {
    rest_for_minutes(
        ctx,
        character_id,
        requested_minutes,
        SettlementRestProvision::PrivateDowntime,
        explicit,
        true,
        None,
    )
    .map(|_| ())
}

/// Adopt a settlement's canonical time of day without adopting its date.
/// Characters advance to the next matching minute within their own subjective
/// day, so this forced reintegration is always shorter than 24 hours. An inn
/// supplies paid full board when available and affordable; otherwise the
/// character receives free church sanctuary.
pub(crate) fn synchronize_to_settlement_time_of_day(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<bool, String> {
    let character = crate::character::require_living_character(ctx, character_id)?;
    if character.current_settlement_id.is_none() {
        return Err("Time-of-day synchronization requires a settlement".into());
    }
    ensure_character_time(ctx, character_id)?;
    let official_minute_of_day = refresh_clock(ctx)? % MINUTES_PER_DAY;
    let character_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or("Character time record not found")?
        .minutes;
    let character_minute_of_day = character_minute % MINUTES_PER_DAY;
    let elapsed = settlement_arrival_downtime(character_minute_of_day, official_minute_of_day);
    if elapsed == 0 {
        return Ok(true);
    }

    let inn_available =
        require_character_rest_service(ctx, character_id, SettlementActionService::Inn).is_ok();
    let inn_affordable =
        crate::item::personal_currency_total(ctx, character_id) >= inn_stay_cost(elapsed)?;
    let provision = if inn_available && inn_affordable {
        SettlementRestProvision::PublicService(SettlementActionService::Inn)
    } else {
        // Arrival sanctuary is a universal settlement fallback, not a
        // player-selected service that can be unavailable.
        SettlementRestProvision::PublicService(SettlementActionService::Temple)
    };
    rest_for_minutes(ctx, character_id, elapsed, provision, false, true, None)?;
    Ok(ctx
        .db
        .character()
        .id()
        .find(character_id)
        .is_some_and(|character| character.alive))
}

fn settlement_arrival_downtime(character_minute_of_day: u64, official_minute_of_day: u64) -> u64 {
    (official_minute_of_day + MINUTES_PER_DAY - character_minute_of_day) % MINUTES_PER_DAY
}

/// Establish a journey-local clock without changing any participant's
/// subjective age. The leader-selected time of day is placed on the canonical
/// departure day; elapsed journey progress later wraps within that frozen day.
pub(crate) fn synchronize_party_departure_time(
    ctx: &ReducerContext,
    member_ids: &[u64],
) -> Result<Option<u64>, String> {
    if member_ids.is_empty() {
        return Err("Party has no living members".into());
    }
    for member_id in member_ids {
        ensure_character_time(ctx, *member_id)?;
    }
    let party_id = member_ids
        .iter()
        .find_map(|member_id| {
            ctx.db
                .character()
                .id()
                .find(*member_id)
                .and_then(|character| character.party_id)
        })
        .ok_or("Party members have no party")?;
    let mut party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.wilderness_canonical_anchor_minute.is_none() {
        party.wilderness_canonical_anchor_minute = Some(refresh_clock(ctx)?);
        party.wilderness_elapsed_minutes = 0;
        ctx.db.party_authority().id().update(party.clone());
    }
    let anchor = party
        .wilderness_canonical_anchor_minute
        .ok_or("Party wilderness clock was not initialized")?;
    let frozen_day = anchor / MINUTES_PER_DAY * MINUTES_PER_DAY;
    let local_minute_of_day = (u64::from(party.journey_start_minute_of_day)
        + party.wilderness_elapsed_minutes)
        % MINUTES_PER_DAY;
    Ok(Some(frozen_day.saturating_add(local_minute_of_day)))
}

/// Companions generated by the strategic layer do not wait for a player to
/// select a rest duration. Once the party reaches a settlement, they use the
/// ordinary settlement-rest path until their wounds are healed. The leader is
/// deliberately excluded: even a temporary leader may be player-controlled in
/// local development.
pub(crate) fn rest_temporary_party_member_until_healed_at_settlement(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<(), String> {
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if !character.temporary || character.current_settlement_id.is_none() {
        return Ok(());
    }
    let Some(party_id) = character.party_id.as_ref() else {
        return Ok(());
    };
    if ctx
        .db
        .party_authority()
        .id()
        .find(party_id)
        .is_some_and(|party| party.leader_id == character_id)
    {
        return Ok(());
    }

    let recovery_minutes = convalescence_minutes(
        ctx,
        character_id,
        party_physiology_check(ctx, character_id)?,
    );
    if recovery_minutes > 0 {
        spend_private_settlement_downtime(ctx, character_id, recovery_minutes, false)?;
    }
    Ok(())
}
