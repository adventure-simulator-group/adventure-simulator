// Owns permitted camp schedules and shared field-rest execution.
pub(crate) fn allowed_camp_schedule(schedule: &ScheduleAllocation) -> ScheduleAllocation {
    let mut allowed = schedule.clone();
    allowed.reading_minutes = 0;
    allowed.apprenticeship_minutes = 0;
    allowed.apprenticeship_organization_id = None;
    allowed.profession_practice_minutes = 0;
    allowed.practice_organization_id = None;
    allowed.labor_minutes = 0;
    allowed.thievery_minutes = 0;
    allowed.raiding_minutes = 0;
    allowed
}

/// Field rest is a party action from the map at a settlement, an en-route camp,
/// or a quest destination: the leader chooses a duration and every party member
/// spends the same strategic time without settlement replenishment or prices.
#[reducer]
pub fn rest_at_camp(
    ctx: &ReducerContext,
    character_id: u64,
    requested_minutes: u64,
    shelter: FieldShelter,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    crate::strategic::require_character_no_unresolved_encounter(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    if requested_minutes == 0 {
        return Ok(());
    }
    if requested_minutes > MINUTES_PER_YEAR {
        return Err("Camp rest cannot exceed one year".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Must be in a party to camp")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if shelter == FieldShelter::Tent
        && !ctx
            .db
            .party_inventory_item()
            .party_id()
            .filter(&party_id)
            .any(|row| {
                row.quantity > 0
                    && adventuresim_core::item_catalog::definition(&row.item_id).is_some_and(
                        |definition| definition.tags.iter().any(|tag| tag == "field_shelter"),
                    )
            })
    {
        return Err("A tent must be in party inventory before choosing tent shelter".into());
    }
    if !crate::strategic::party_member_can_direct_field_rest(ctx, &party, character_id) {
        return Err(
            "Only the party leader, or a ready companion aiding an unready leader, can rest the party at camp"
                .into(),
        );
    }
    if party.current_settlement_id.is_none()
        && party.camp_destination.is_none()
        && party.current_case_site_id.is_none()
    {
        return Err("The party is not at a field rest location".into());
    }
    let narrative_authority = ctx
        .db
        .party_journey_encounter_authority()
        .party_id()
        .find(&party_id);
    let pending_narrative =
        if party.current_settlement_id.is_none() && party.camp_destination.is_some() {
            narrative_authority.as_ref().and_then(|authority| {
                adventuresim_core::encounter::first_narrative_encounter(
                    authority.seed,
                    authority.narrative_rest_elapsed_minutes,
                    requested_minutes,
                    adventuresim_core::encounter::NarrativeContext {
                        kind: adventuresim_core::encounter::NarrativeBoundaryKind::Rest,
                        in_settlement: false,
                        another_interruption_pending: ctx
                            .db
                            .road_challenge_authority()
                            .party_id()
                            .filter(&party_id)
                            .any(|occurrence| occurrence.open),
                    },
                )
            })
        } else {
            None
        };
    let requested_minutes = pending_narrative
        .as_ref()
        .map_or(requested_minutes, |selection| {
            selection.boundary_minute.saturating_sub(
                narrative_authority
                    .as_ref()
                    .map_or(0, |authority| authority.narrative_rest_elapsed_minutes),
            )
        });
    let members = crate::strategic::living_party_member_ids(ctx, &party_id);
    // This reducer is an explicit player-chosen rest. Washing precedes disease
    // and injury interval clipping, and dead members were excluded above.
    crate::filth::wash_party_before_explicit_rest(ctx, &members)?;
    let disease_plan =
        crate::disease::plan_party_disease_interval(ctx, &members, requested_minutes, true)?;
    let elapsed = members
        .iter()
        .try_fold(requested_minutes, |limit, member_id| {
            let disease = crate::disease::preview_elapsed_for_disease_in_plan(
                ctx,
                *member_id,
                limit,
                true,
                &disease_plan,
            )?;
            let injury = crate::surgery::preview_injury_boundary(
                ctx,
                *member_id,
                limit,
                InjuryRecoveryMinutes::new(limit),
            )?;
            Ok::<u64, String>(limit.min(disease).min(injury.elapsed))
        })?;
    let fatigue_before = party_fatigue_summary(ctx, &members)?;
    let language_snapshot: Vec<_> = members
        .iter()
        .filter_map(|id| {
            ctx.db
                .character_skills()
                .character_id()
                .find(*id)
                .map(|skills| {
                    let cap = ctx
                        .db
                        .character_attributes()
                        .character_id()
                        .find(*id)
                        .map_or(0.0, |attributes| attributes.instinct * 1_000.0);
                    (*id, skills.oral_languages, cap)
                })
        })
        .collect();
    let language_choices: BTreeMap<_, _> =
        adventuresim_world_schema::party_common_oral_choices_capped(&language_snapshot)
            .into_iter()
            .map(|(id, language, coefficient)| (id, (language, coefficient)))
            .collect();
    let mut automatic_chat_downtime = Vec::new();
    for member_id in members {
        ensure_character_time(ctx, member_id)?;
        let mut time = ctx
            .db
            .character_time()
            .character_id()
            .find(member_id)
            .ok_or("Character time record not found")?;
        let starting_fatigue = ctx
            .db
            .character_stats()
            .character_id()
            .find(member_id)
            .map_or(0.0, |stats| stats.calories_used.max(0.0));
        let physiology_check = party_physiology_check(ctx, member_id)?;
        let convalescing = convalescence_minutes(ctx, member_id, physiology_check).min(elapsed);
        let (disease_elapsed, terminal) = crate::disease::clip_elapsed_for_disease_in_plan(
            ctx,
            member_id,
            elapsed,
            true,
            &disease_plan,
        )?;
        let injury_elapsed = elapsed.min(disease_elapsed);
        let settled = crate::surgery::settle_injuries(
            ctx,
            member_id,
            injury_elapsed,
            InjuryRecoveryMinutes::new(injury_elapsed),
        )?;
        let member_elapsed = settled.elapsed;
        time.minutes = time.minutes.saturating_add(member_elapsed);
        let interval_end_minute = time.minutes;
        ctx.db.character_time().character_id().update(time);
        advance_married_family_by(ctx, member_id, member_elapsed)?;
        crate::condition::apply_weather_exposure(
            ctx,
            member_id,
            interval_end_minute.saturating_sub(member_elapsed),
            member_elapsed,
            false,
            ExposureShelter::Field(shelter),
        )?;
        crate::organization::settle_membership_dues(ctx, member_id)?;
        crate::social::settle_shared_party_time(ctx, member_id);
        crate::condition::apply_elapsed_needs(ctx, member_id, member_elapsed)?;
        crate::disease::finish_disease_interval(ctx, member_id, terminal)?;
        settle_lifecycle_after_character_time_write(ctx, member_id, interval_end_minute)?;
        if terminal.is_some() || !settled.alive {
            continue;
        }
        crate::alcohol::process_rest_evenings(
            ctx,
            member_id,
            interval_end_minute.saturating_sub(member_elapsed),
            interval_end_minute,
            false,
        )?;
        crate::condition::apply_camp_rest_recovery_condition(ctx, member_id, member_elapsed)?;
        crate::food::clear_stomach_fullness(ctx, member_id);
        let convalescing = convalescing.min(member_elapsed);
        let attributes = ctx
            .db
            .character_attributes()
            .character_id()
            .find(member_id)
            .ok_or("Character attributes not found")?;
        let (smithing_skill, tailoring_skill) = ctx
            .db
            .character_skills()
            .character_id()
            .find(member_id)
            .map(|skills| {
                (
                    Skill::Smithing
                        .capped_training_rank(skills.smithing_hours, &attributes)
                        .floor() as u8,
                    Skill::Tailoring
                        .capped_training_rank(skills.tailoring_hours, &attributes)
                        .floor() as u8,
                )
            })
            .unwrap_or((0, 0));
        let maintenance = crate::repair::field_repair(
            ctx,
            member_id,
            smithing_skill,
            tailoring_skill,
            adventuresim_core::durability::remaining_after_priority(member_elapsed, convalescing),
        );
        let fatigue_rest =
            adventuresim_core::strategic_time::minutes_until_fatigue_clears(starting_fatigue)
                .min(member_elapsed);
        let priority = fatigue_rest.max(convalescing.saturating_add(maintenance));
        let downtime = member_elapsed.saturating_sub(priority);
        if downtime > 0 {
            let schedule = ctx
                .db
                .character_training_schedule()
                .character_id()
                .find(member_id)
                .ok_or("Character training schedule not found")?;
            let allowed = effective_location_schedule(
                &allowed_camp_schedule(&schedule.downtime),
                ActivityLocation::JourneyCamp,
                member_id,
            );
            let mut skills = ctx
                .db
                .character_skills()
                .character_id()
                .find(member_id)
                .ok_or("Character skill record not found")?;
            let activities = activity_training_profile(ctx, member_id)?;
            let mut excess =
                apply_training(ctx, member_id, &mut skills, &allowed, downtime, activities)?;
            if let Some((language, coefficient)) = language_choices.get(&member_id) {
                excess += apply_oral_language_training(
                    ctx,
                    member_id,
                    &mut skills.oral_languages,
                    *language,
                    downtime as f32 / 60.0 * (2.0 / 3.0) * coefficient,
                );
            }
            crate::condition::record_mastery_training_morale(ctx, member_id, downtime, excess);
            ctx.db.character_skills().character_id().update(skills);
            crate::condition::apply_settlement_leisure_condition(
                ctx,
                member_id,
                core_schedule(&allowed),
                downtime,
                interval_end_minute,
            )?;
            automatic_chat_downtime.push((member_id, downtime));
        }
        crate::capability::refresh_character_capability(ctx, member_id)?;
    }
    // Resolve chats after every member's clock has reached the end of the
    // shared interval so target-clock cooldowns receive the full cadence.
    automatic_chat_downtime.sort_by_key(|(member_id, _)| *member_id);
    for (member_id, downtime) in automatic_chat_downtime {
        crate::social::apply_automatic_social_chats(ctx, member_id, downtime)?;
    }
    let living_after = crate::strategic::living_party_member_ids(ctx, &party_id);
    if living_after.is_empty() {
        crate::strategic::teardown_all_dead_strategic_party(ctx, &party_id)?;
        return Ok(());
    }
    let fatigue_after = party_fatigue_summary(ctx, &living_after)?;
    crate::strategic::record_party_camp_rest(
        ctx,
        &party_id,
        elapsed,
        fatigue_before.0,
        fatigue_after.0,
        fatigue_after.1,
    )?;
    if let Some(mut authority) = narrative_authority {
        authority.narrative_rest_elapsed_minutes = authority
            .narrative_rest_elapsed_minutes
            .saturating_add(elapsed);
        let reached = pending_narrative.as_ref().is_some_and(|selection| {
            selection.boundary_minute == authority.narrative_rest_elapsed_minutes
        });
        ctx.db
            .party_journey_encounter_authority()
            .party_id()
            .update(authority);
        if reached && let Some(narrative) = pending_narrative.as_ref() {
            crate::strategic::materialize_chance_narrative_encounter(
                ctx,
                &party_id,
                narrative,
                crate::strategic::NarrativeEncounterOrigin::ChanceRest,
            )?;
        }
    }
    // Reforecast the untravelled part from the fatigue that this particular
    // rest actually removed. The journey record retains all reached camps.
    crate::strategic::refresh_party_journey_forecast(ctx, &party_id)?;
    crate::strategic::reconcile_party_objective_continuity(ctx, &party_id)?;
    Ok(())
}

fn party_fatigue_summary(ctx: &ReducerContext, members: &[u64]) -> Result<(f32, f32), String> {
    if members.is_empty() {
        return Ok((0.0, 0.0));
    }
    let mut total = 0.0;
    let mut maximum = 0.0_f32;
    for member_id in members {
        let attributes = ctx
            .db
            .character_attributes()
            .character_id()
            .find(*member_id)
            .ok_or("Party member attributes not found")?;
        let limbs = ctx
            .db
            .character_limbs()
            .character_id()
            .find(*member_id)
            .ok_or("Party member limbs not found")?;
        let stats = ctx
            .db
            .character_stats()
            .character_id()
            .find(*member_id)
            .ok_or("Party member stats not found")?;
        let capacity = attributes
            .attr_by_parts(SimpleAttribute::Endurance, &limbs)
            .max(0.01)
            * 1_000.0;
        let fatigue = stats.calories_used.max(0.0) / capacity;
        total += fatigue;
        maximum = maximum.max(fatigue);
    }
    Ok((total / members.len() as f32, maximum))
}
