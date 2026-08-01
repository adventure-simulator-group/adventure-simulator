/// Return the next leg's length. The least-rested member sets the party's
/// pace: once that member reaches the configured raw fatigue percentage, the
/// party makes camp. A one-minute minimum lets an already-tired party begin a
/// journey and immediately establish camp rather than becoming stranded.
fn party_travel_leg_minutes(
    ctx: &ReducerContext,
    party_id: &str,
    _fatigue_percent: u8,
) -> Result<u64, String> {
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id.to_string())
        .ok_or("Party not found")?;
    if party.walking_minutes_per_day == 0 {
        return Err("The party is configured not to travel".into());
    }
    if daylight_walking_window(party.walking_minutes_per_day).is_none() {
        return Err("Party walking hours are invalid".into());
    }
    Ok(u64::from(party.walking_minutes_per_day))
}

fn party_next_walking_minutes(
    ctx: &ReducerContext,
    party_id: &str,
    remaining_movement: u64,
) -> Result<u64, String> {
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id.to_string())
        .ok_or("Party not found")?;
    let now = living_party_member_ids(ctx, party_id)
        .into_iter()
        .filter_map(|id| ctx.db.character_time().character_id().find(id))
        .map(|time| time.minutes)
        .max()
        .unwrap_or(0);
    let itinerary = forecast_itinerary(
        now,
        remaining_movement,
        party.walking_minutes_per_day,
        party.travel_at_night,
        party_camp_policy(&party),
        &party_itinerary_members(ctx, party_id)?,
    )
    .ok_or("Unable to forecast the next travel leg")?;
    Ok(itinerary.segments.first().map_or(0, |segment| {
        if matches!(segment.kind, ItinerarySegmentKind::Walking) {
            segment.movement_minutes
        } else {
            0
        }
    }))
}

fn full_rest_party_travel_leg_minutes(
    ctx: &ReducerContext,
    party_id: &str,
    fatigue_percent: u8,
) -> Result<u64, String> {
    party_travel_leg_minutes(ctx, party_id, fatigue_percent)
}

fn party_camp_policy(party: &Party) -> CampDurationPolicy {
    match party.camp_duration_mode {
        CampDurationMode::Auto => CampDurationPolicy::Auto,
        CampDurationMode::Fixed => CampDurationPolicy::FixedMinutes(party.fixed_camp_minutes),
    }
}

fn party_itinerary_members(
    ctx: &ReducerContext,
    party_id: &str,
) -> Result<Vec<ItineraryMember>, String> {
    let mut members = Vec::new();
    for member_id in living_party_member_ids(ctx, party_id) {
        let attributes = ctx
            .db
            .character_attributes()
            .character_id()
            .find(member_id)
            .ok_or("Party member attributes not found")?;
        let limbs = ctx
            .db
            .character_limbs()
            .character_id()
            .find(member_id)
            .ok_or("Party member limbs not found")?;
        let stats = ctx
            .db
            .character_stats()
            .character_id()
            .find(member_id)
            .ok_or("Party member stats not found")?;
        let schedule = ctx
            .db
            .character_training_schedule()
            .character_id()
            .find(member_id)
            .ok_or("Party member schedule not found")?;
        let allowed = crate::time::effective_location_schedule(
            &crate::time::allowed_camp_schedule(&schedule.downtime),
            adventuresim_core::activity::ActivityLocation::JourneyCamp,
            member_id,
        );
        members.push(ItineraryMember {
            fatigue_capacity: attributes
                .attr_by_parts(SimpleAttribute::Endurance, &limbs)
                .max(0.01)
                * 1_000.0,
            calories_used: stats.calories_used.max(0.0),
            camp_schedule: crate::time::core_schedule(&allowed),
        });
    }
    Ok(members)
}

fn itinerary_camps(forecast: &ItineraryForecast) -> Vec<JourneyCampInterval> {
    let mut camps: Vec<JourneyCampInterval> = Vec::new();
    for segment in forecast
        .segments
        .iter()
        .filter(|segment| segment.kind == ItinerarySegmentKind::Camp)
    {
        if let Some(last) = camps.last_mut()
            && last.movement_minute == segment.movement_start
            && last
                .elapsed_start_minute
                .saturating_add(last.elapsed_minutes)
                == segment.elapsed_start
        {
            last.elapsed_minutes = last.elapsed_minutes.saturating_add(segment.elapsed_minutes);
            last.average_fatigue_end = segment.average_fatigue_end;
            last.maximum_fatigue_end = last.maximum_fatigue_end.max(segment.maximum_fatigue_end);
            continue;
        }
        if camps.len() >= MAX_ITINERARY_SEGMENTS {
            break;
        }
        camps.push(JourneyCampInterval {
            movement_minute: segment.movement_start,
            elapsed_start_minute: segment.elapsed_start,
            elapsed_minutes: segment.elapsed_minutes,
            average_fatigue_start: segment.average_fatigue_start,
            average_fatigue_end: segment.average_fatigue_end,
            maximum_fatigue_end: segment.maximum_fatigue_end,
        });
    }
    camps
}

fn forecast_camp_stop_minutes(
    ctx: &ReducerContext,
    party_id: &str,
    total_minutes: u64,
    completed_minutes: u64,
    fatigue_percent: u8,
) -> Result<Vec<u64>, String> {
    let mut stops = Vec::new();
    let mut elapsed = completed_minutes.min(total_minutes);
    let mut use_current_fatigue = true;
    while elapsed < total_minutes {
        let leg_minutes = if use_current_fatigue {
            party_travel_leg_minutes(ctx, party_id, fatigue_percent)?
        } else {
            full_rest_party_travel_leg_minutes(ctx, party_id, fatigue_percent)?
        };
        elapsed = elapsed.saturating_add(leg_minutes).min(total_minutes);
        if elapsed < total_minutes {
            if stops.len() >= MAX_ITINERARY_SEGMENTS {
                return Err("Journey requires too many legacy camp checkpoints".into());
            }
            stops.push(elapsed);
        }
        use_current_fatigue = false;
    }
    Ok(stops)
}

fn start_party_journey(
    ctx: &ReducerContext,
    party: &Party,
    origin: JourneyEndpoint,
    destination: JourneyEndpoint,
    total_minutes: u64,
    departure_minute: u64,
    route: Option<&JourneyRoutePlan>,
) -> Result<(), String> {
    require_no_unresolved_encounter(ctx, &party.id)?;
    if ctx
        .db
        .strategic_encounter()
        .party_id()
        .find(&party.id)
        .is_some()
    {
        ctx.db.strategic_encounter().party_id().delete(&party.id);
    }
    if ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party.id)
        .is_some()
    {
        ctx.db
            .party_journey_authority()
            .party_id()
            .delete(&party.id);
    }
    if ctx
        .db
        .party_journey_itinerary()
        .party_id()
        .find(&party.id)
        .is_some()
    {
        ctx.db
            .party_journey_itinerary()
            .party_id()
            .delete(&party.id);
    }
    if ctx
        .db
        .party_journey_route_authority()
        .party_id()
        .find(&party.id)
        .is_some()
    {
        ctx.db
            .party_journey_route_authority()
            .party_id()
            .delete(&party.id);
    }
    let fatigue_percent = party.camp_fatigue_percent;
    let forecast_camp_stop_minutes =
        forecast_camp_stop_minutes(ctx, &party.id, total_minutes, 0, fatigue_percent)?;
    // This authority describes the active leg only. A later return starts its
    // own journey and itinerary; including a speculative return here doubles
    // camp exposure and disagrees with `total_minutes` progress.
    let planned_movement = total_minutes;
    let itinerary = forecast_itinerary(
        departure_minute,
        planned_movement,
        party.walking_minutes_per_day,
        party.travel_at_night,
        party_camp_policy(party),
        &party_itinerary_members(ctx, &party.id)?,
    )
    .ok_or("Unable to forecast the party itinerary")?;
    if itinerary.truncated {
        return Err("Journey requires too many itinerary checkpoints".into());
    }
    // Actual departure ends any uninterrupted site/protection interval before
    // movement time is committed. Re-entering can create a new guard, but
    // never retroactively repairs this one.
    break_party_objective_continuity(ctx, &party.id)?;
    ctx.db.party_journey_authority().insert(PartyJourney {
        party_id: party.id.clone(),
        gateway_bucket: 0,
        origin,
        destination,
        total_minutes,
        completed_minutes: 0,
        camp_stop_minutes: Vec::new(),
        forecast_camp_stop_minutes,
        fatigue_percent,
        plan_version: 1,
        departure_minute,
        total_elapsed_minutes: itinerary.total_elapsed_minutes,
        completed_elapsed_minutes: 0,
        walking_minutes_per_day: party.walking_minutes_per_day,
        travel_at_night: party.travel_at_night,
        camp_duration_mode: party.camp_duration_mode,
        fixed_camp_minutes: party.fixed_camp_minutes,
    });
    ctx.db
        .party_journey_encounter_authority()
        .insert(PartyJourneyEncounterAuthority {
            party_id: party.id.clone(),
            seed: ctx.random(),
            next_roll: 1,
            narrative_rest_elapsed_minutes: 0,
        });
    ctx.db
        .party_journey_itinerary()
        .insert(PartyJourneyItinerary {
            party_id: party.id.clone(),
            actual_camp_intervals: Vec::new(),
            forecast_camp_intervals: itinerary_camps(&itinerary),
        });
    if let Some(route) = route {
        ctx.db
            .party_journey_route_authority()
            .insert(PartyJourneyRoute {
                party_id: party.id.clone(),
                gateway_bucket: 0,
                package_digest: route.package_digest.clone(),
                weather_rules_version: route.weather_rules_version,
                weather_interval_start: route.weather_interval_start,
                precipitation: route.precipitation,
                intensity_bps: route.intensity_bps,
                ground_moisture_bps: route.ground_moisture_bps,
                snow_cover_bps: route.snow_cover_bps,
                distance_m: route.distance_m,
                minutes: route.minutes,
                points: route.points.clone(),
                spans: route.spans.clone(),
                return_route: route.return_route.clone(),
            });
    }
    Ok(())
}

fn record_party_journey_camp(
    ctx: &ReducerContext,
    party_id: &str,
    leg_minutes: u64,
) -> Result<(), String> {
    let Some(mut journey) = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party_id.to_string())
    else {
        return Ok(());
    };
    journey.completed_minutes = journey
        .completed_minutes
        .saturating_add(leg_minutes)
        .min(journey.total_minutes);
    journey.completed_elapsed_minutes = journey
        .completed_elapsed_minutes
        .saturating_add(leg_minutes);
    if journey.camp_stop_minutes.last() != Some(&journey.completed_minutes) {
        journey.camp_stop_minutes.push(journey.completed_minutes);
    }
    ctx.db.party_journey_authority().party_id().update(journey);
    bind_errantry_trials_to_current_camp(ctx, party_id)?;
    Ok(())
}

fn record_party_journey_interruption(ctx: &ReducerContext, party_id: &str, movement_minutes: u64) {
    if let Some(mut journey) = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party_id.to_string())
    {
        journey.completed_minutes = journey
            .completed_minutes
            .saturating_add(movement_minutes)
            .min(journey.total_minutes);
        journey.completed_elapsed_minutes = journey
            .completed_elapsed_minutes
            .saturating_add(movement_minutes);
        ctx.db.party_journey_authority().party_id().update(journey);
    }
}

/// Award conserved terrain exposure for the exact movement interval about to
/// be advanced. Camp time never reaches this function. The persisted route is
/// the departure snapshot, so chunked/offline continuation cannot change the
/// check, duration, or skill mixture mid-journey.
fn train_party_terrain_movement(
    ctx: &ReducerContext,
    party_id: &str,
    movement_minutes: u64,
) -> Result<std::collections::BTreeMap<u64, f32>, String> {
    let mut excess_by_character = std::collections::BTreeMap::new();
    if movement_minutes == 0 {
        return Ok(excess_by_character);
    }
    let Some(journey) = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party_id.to_string())
    else {
        return Ok(excess_by_character);
    };
    let Some(route) = ctx
        .db
        .party_journey_route_authority()
        .party_id()
        .find(&party_id.to_string())
    else {
        return Ok(excess_by_character);
    };
    let start = journey.completed_minutes;
    let end = start.saturating_add(movement_minutes).min(route.minutes);
    let exposure = terrain_training_exposure(&route.spans, start, end, route.snow_cover_bps);
    for member_id in living_party_member_ids(ctx, party_id) {
        if let Some(mut skills) = ctx.db.character_skills().character_id().find(member_id) {
            let attributes = ctx
                .db
                .character_attributes()
                .character_id()
                .find(member_id)
                .ok_or("Character attributes not found")?;
            let mut excess = 0.0;
            for (stored, skill, real_hours) in [
                (
                    &mut skills.terrain_plains_hours,
                    Skill::TerrainPlains,
                    exposure[0],
                ),
                (
                    &mut skills.terrain_forest_hours,
                    Skill::TerrainForest,
                    exposure[1],
                ),
                (
                    &mut skills.terrain_hills_hours,
                    Skill::TerrainHills,
                    exposure[2],
                ),
                (
                    &mut skills.terrain_wetlands_hours,
                    Skill::TerrainWetlands,
                    exposure[3],
                ),
                (
                    &mut skills.terrain_urban_hours,
                    Skill::TerrainUrban,
                    exposure[4],
                ),
                (
                    &mut skills.terrain_snow_hours,
                    Skill::TerrainSnow,
                    exposure[5],
                ),
            ] {
                excess += adventuresim_core::skill::apply_direct_training(
                    skill,
                    stored,
                    real_hours,
                    &attributes,
                )
                .excess_effective_hours;
            }
            ctx.db.character_skills().character_id().update(skills);
            excess_by_character.insert(member_id, excess);
        }
    }
    Ok(excess_by_character)
}

/// Give each traveler at most one interval of conversational exposure. Choices
/// are made from one sorted pre-gain snapshot, so party iteration cannot affect
/// the result and additional companions cannot multiply elapsed time.
fn train_party_oral_communication(
    ctx: &ReducerContext,
    party_id: &str,
    movement_minutes: u64,
) -> std::collections::BTreeMap<u64, f32> {
    let mut excess_by_character = std::collections::BTreeMap::new();
    if movement_minutes == 0 {
        return excess_by_character;
    }
    let mut snapshot: Vec<_> = living_party_member_ids(ctx, party_id)
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
    snapshot.sort_by_key(|(id, _, _)| *id);
    let interval_hours = movement_minutes as f32 / 60.0;
    let gains =
        adventuresim_world_schema::party_oral_training_gains_capped(&snapshot, interval_hours);
    for (id, language, hours) in gains {
        if let Some(mut skills) = ctx.db.character_skills().character_id().find(id) {
            let instinct = ctx
                .db
                .character_attributes()
                .character_id()
                .find(id)
                .map_or(0.0, |attributes| attributes.instinct);
            let excess = adventuresim_core::skill::apply_language_training(
                skills.oral_languages.direct_mut(language),
                hours,
                instinct,
            )
            .excess_effective_hours;
            ctx.db.character_skills().character_id().update(skills);
            excess_by_character.insert(id, excess);
        }
    }
    excess_by_character
}

fn terrain_training_exposure(
    spans: &[JourneyTerrainSpan],
    start: u64,
    end: u64,
    snow_cover_bps: u16,
) -> [f32; 6] {
    let mut exposure = [0.0_f32; 6];
    for span in spans {
        let overlap = end
            .min(span.start_minute.saturating_add(span.duration_minutes))
            .saturating_sub(start.max(span.start_minute));
        if overlap == 0 {
            continue;
        }
        let hours = overlap as f32 / 60.0 * f32::from(span.training_multiplier_permille) / 1_000.0;
        let snow_share = f32::from(snow_cover_bps.min(10_000)) / 10_000.0;
        let underlying_hours = hours * (1.0 - snow_share);
        exposure[0] += underlying_hours * f32::from(span.terrain.plains) / 1_000.0;
        exposure[1] += underlying_hours * f32::from(span.terrain.forest) / 1_000.0;
        exposure[2] += underlying_hours * f32::from(span.terrain.hills) / 1_000.0;
        exposure[3] += underlying_hours * f32::from(span.terrain.wetlands) / 1_000.0;
        exposure[4] += underlying_hours * f32::from(span.terrain.urban) / 1_000.0;
        exposure[5] += hours * snow_share;
    }
    exposure
}

fn advance_party_movement(
    ctx: &ReducerContext,
    party_id: &str,
    traveler_ids: &[u64],
    requested_minutes: u64,
) -> Result<(u64, bool), String> {
    let disease_plan =
        crate::disease::plan_party_disease_interval(ctx, traveler_ids, requested_minutes, false)?;
    let mut safe_prefixes = Vec::with_capacity(traveler_ids.len());
    for member_id in traveler_ids {
        safe_prefixes.push(crate::time::preview_travel_time_in_plan(
            ctx,
            *member_id,
            requested_minutes,
            &disease_plan,
        )?);
    }
    let actual_minutes = common_movement_prefix(requested_minutes, safe_prefixes.iter().copied());
    if actual_minutes == 0 {
        let mut all_survived = true;
        for (member_id, safe_prefix) in traveler_ids.iter().zip(safe_prefixes) {
            if zero_boundary_requires_settlement(actual_minutes, safe_prefix) {
                all_survived &= settle_travel_boundary(ctx, *member_id)?;
            }
        }
        return Ok((0, all_survived));
    }
    let mut all_survived = true;
    for member_id in traveler_ids.iter().copied() {
        all_survived &= crate::time::advance_travel_time_in_plan(
            ctx,
            member_id,
            actual_minutes,
            &disease_plan,
        )?;
    }
    // Training is committed only after every participant's authoritative
    // clock has committed the same safe movement prefix.
    let mut mastery_excess = train_party_terrain_movement(ctx, party_id, actual_minutes)?;
    for (character_id, excess) in train_party_oral_communication(ctx, party_id, actual_minutes) {
        *mastery_excess.entry(character_id).or_default() += excess;
    }
    for (character_id, excess) in mastery_excess {
        crate::condition::record_mastery_training_morale(ctx, character_id, actual_minutes, excess);
    }
    Ok((actual_minutes, all_survived))
}

fn zero_boundary_requires_settlement(actual_minutes: u64, safe_prefix: u64) -> bool {
    actual_minutes == 0 && safe_prefix == 0
}

fn set_party_journey_state(
    party: &mut Party,
    current_settlement_id: Option<String>,
    current_case_site_id: Option<CaseSiteId>,
    camp_destination: Option<JourneyEndpoint>,
    camp_remaining_minutes: u64,
) {
    // Deliberately touch only journey fields. In particular, leadership may
    // have changed while movement committed a terminal event.
    party.current_settlement_id = current_settlement_id;
    party.current_case_site_id = current_case_site_id;
    party.camp_destination = camp_destination;
    party.camp_remaining_minutes = camp_remaining_minutes;
}

fn party_can_continue_travel(party: &Party, character_id: u64) -> bool {
    party.leader_id == character_id
}

fn common_movement_prefix(
    requested_minutes: u64,
    safe_prefixes: impl IntoIterator<Item = u64>,
) -> u64 {
    safe_prefixes.into_iter().fold(requested_minutes, u64::min)
}

pub(crate) fn refresh_party_journey_forecast(
    ctx: &ReducerContext,
    party_id: &str,
) -> Result<(), String> {
    let Some(mut journey) = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party_id.to_string())
    else {
        return Ok(());
    };
    if journey.plan_version == 0 {
        let current = living_party_member_ids(ctx, party_id)
            .into_iter()
            .filter_map(|id| ctx.db.character_time().character_id().find(id))
            .map(|time| time.minutes)
            .max()
            .unwrap_or(0);
        (journey.departure_minute, journey.completed_elapsed_minutes) =
            reconstruct_legacy_journey_coordinates(current, journey.completed_minutes);
        journey.plan_version = 1;
    }
    journey.forecast_camp_stop_minutes = forecast_camp_stop_minutes(
        ctx,
        party_id,
        journey.total_minutes,
        journey.completed_minutes,
        journey.fatigue_percent,
    )?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id.to_string())
        .ok_or("Party not found")?;
    let start = journey
        .departure_minute
        .saturating_add(journey.completed_elapsed_minutes);
    // A persisted journey always describes its active leg. A return trip is a
    // new journey and must not be folded into a refreshed outbound forecast.
    let planned_movement = journey.total_minutes;
    let remaining = planned_movement.saturating_sub(journey.completed_minutes);
    let itinerary = forecast_itinerary(
        start,
        remaining,
        party.walking_minutes_per_day,
        party.travel_at_night,
        party_camp_policy(&party),
        &party_itinerary_members(ctx, party_id)?,
    )
    .ok_or("Unable to forecast the remaining itinerary")?;
    if itinerary.truncated {
        return Err("Journey requires too many itinerary checkpoints".into());
    }
    journey.walking_minutes_per_day = party.walking_minutes_per_day;
    journey.travel_at_night = party.travel_at_night;
    journey.camp_duration_mode = party.camp_duration_mode;
    journey.fixed_camp_minutes = party.fixed_camp_minutes;
    journey.total_elapsed_minutes = journey
        .completed_elapsed_minutes
        .saturating_add(itinerary.total_elapsed_minutes);
    let forecast_camp_intervals = itinerary_camps(&itinerary)
        .into_iter()
        .map(|mut interval| {
            interval.movement_minute = interval
                .movement_minute
                .saturating_add(journey.completed_minutes);
            interval.elapsed_start_minute = interval
                .elapsed_start_minute
                .saturating_add(journey.completed_elapsed_minutes);
            interval
        })
        .collect();
    let mut typed = ctx
        .db
        .party_journey_itinerary()
        .party_id()
        .find(&party_id.to_string())
        .unwrap_or(PartyJourneyItinerary {
            party_id: party_id.to_string(),
            actual_camp_intervals: Vec::new(),
            forecast_camp_intervals: Vec::new(),
        });
    typed.forecast_camp_intervals = forecast_camp_intervals;
    if ctx
        .db
        .party_journey_itinerary()
        .party_id()
        .find(&party_id.to_string())
        .is_some()
    {
        ctx.db.party_journey_itinerary().party_id().update(typed);
    } else {
        ctx.db.party_journey_itinerary().insert(typed);
    }
    ctx.db.party_journey_authority().party_id().update(journey);
    Ok(())
}

pub(crate) fn record_party_camp_rest(
    ctx: &ReducerContext,
    party_id: &str,
    elapsed: u64,
    average_start: f32,
    average_end: f32,
    maximum_end: f32,
) -> Result<(), String> {
    let Some(mut journey) = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party_id.to_string())
    else {
        return Ok(());
    };
    let start = journey.completed_elapsed_minutes;
    journey.completed_elapsed_minutes = journey.completed_elapsed_minutes.saturating_add(elapsed);
    let mut typed = ctx
        .db
        .party_journey_itinerary()
        .party_id()
        .find(&party_id.to_string())
        .unwrap_or(PartyJourneyItinerary {
            party_id: party_id.to_string(),
            actual_camp_intervals: Vec::new(),
            forecast_camp_intervals: Vec::new(),
        });
    let typed_exists = ctx
        .db
        .party_journey_itinerary()
        .party_id()
        .find(&party_id.to_string())
        .is_some();
    if let Some(last) = typed.actual_camp_intervals.last_mut()
        && last.movement_minute == journey.completed_minutes
        && last
            .elapsed_start_minute
            .saturating_add(last.elapsed_minutes)
            == start
    {
        last.elapsed_minutes = last.elapsed_minutes.saturating_add(elapsed);
        last.average_fatigue_end = average_end;
        last.maximum_fatigue_end = maximum_end;
    } else if typed.actual_camp_intervals.len() < MAX_ITINERARY_SEGMENTS {
        typed.actual_camp_intervals.push(JourneyCampInterval {
            movement_minute: journey.completed_minutes,
            elapsed_start_minute: start,
            elapsed_minutes: elapsed,
            average_fatigue_start: average_start,
            average_fatigue_end: average_end,
            maximum_fatigue_end: maximum_end,
        });
    } else {
        return Err("Journey has too many camp checkpoints".into());
    }
    if typed_exists {
        ctx.db.party_journey_itinerary().party_id().update(typed);
    } else {
        ctx.db.party_journey_itinerary().insert(typed);
    }
    ctx.db.party_journey_authority().party_id().update(journey);
    Ok(())
}

fn finish_party_journey(ctx: &ReducerContext, party_id: &str) {
    let party_id = party_id.to_string();
    if ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party_id)
        .is_some()
    {
        ctx.db
            .party_journey_authority()
            .party_id()
            .delete(&party_id);
    }
    if ctx
        .db
        .party_journey_encounter_authority()
        .party_id()
        .find(&party_id)
        .is_some()
    {
        ctx.db
            .party_journey_encounter_authority()
            .party_id()
            .delete(&party_id);
    }
    if ctx
        .db
        .party_journey_route_authority()
        .party_id()
        .find(&party_id)
        .is_some()
    {
        ctx.db
            .party_journey_route_authority()
            .party_id()
            .delete(&party_id);
    }
    if ctx
        .db
        .party_journey_itinerary()
        .party_id()
        .find(&party_id)
        .is_some()
    {
        ctx.db
            .party_journey_itinerary()
            .party_id()
            .delete(&party_id);
    }
}

fn camp_redirect_minutes(journey: &PartyJourney, settlement_id: &str) -> Option<u64> {
    if journey.origin.settlement_id() == Some(settlement_id) {
        return Some(journey.completed_minutes);
    }
    if journey.destination.settlement_id() == Some(settlement_id) {
        return Some(
            journey
                .total_minutes
                .saturating_sub(journey.completed_minutes),
        );
    }
    None
}

pub(crate) fn route_position_at_minute(
    route: &PartyJourneyRoute,
    minute: u64,
) -> Option<(f64, f64)> {
    let coordinate = |point: &JourneyRoutePoint| {
        (
            f64::from(point.longitude_e7) / 10_000_000.0,
            f64::from(point.latitude_e7) / 10_000_000.0,
        )
    };
    let lengths = route
        .points
        .windows(2)
        .map(|pair| {
            let from = coordinate(&pair[0]);
            let to = coordinate(&pair[1]);
            straight_line_distance_m(from.0, from.1, to.0, to.1, true)
        })
        .collect::<Vec<_>>();
    let total = lengths.iter().sum::<u64>();
    if total == 0 || route.minutes == 0 {
        return route.points.first().map(coordinate);
    }
    let target = total.saturating_mul(minute.min(route.minutes)) / route.minutes;
    let mut traversed = 0_u64;
    for (index, length) in lengths.into_iter().enumerate() {
        if traversed.saturating_add(length) >= target {
            let from = coordinate(&route.points[index]);
            let to = coordinate(&route.points[index + 1]);
            let fraction = if length == 0 {
                0.0
            } else {
                (target.saturating_sub(traversed)) as f64 / length as f64
            };
            return Some((
                from.0 + (to.0 - from.0) * fraction,
                from.1 + (to.1 - from.1) * fraction,
            ));
        }
        traversed = traversed.saturating_add(length);
    }
    route.points.last().map(coordinate)
}

fn unresolved_encounter(ctx: &ReducerContext, party_id: &str) -> Option<StrategicEncounter> {
    ctx.db
        .strategic_encounter()
        .party_id()
        .find(&party_id.to_string())
        .filter(|encounter| encounter.status == "awaiting_choice")
}

pub(crate) fn require_no_unresolved_encounter(
    ctx: &ReducerContext,
    party_id: &str,
) -> Result<(), String> {
    let party = ctx.db.party_authority().id().find(&party_id.to_string());
    let narrative_pending = party.as_ref().is_some_and(|party| {
        ctx.db.road_challenge_authority().party_id().filter(&party_id.to_string())
            .any(|occurrence| occurrence.open && party_at_bound_road_challenge(ctx, party, &occurrence))
    });
    if unresolved_encounter(ctx, party_id).is_some() || narrative_pending {
        Err("Resolve the strategic encounter before changing or continuing travel".into())
    } else {
        Ok(())
    }
}

pub(crate) fn require_character_no_unresolved_encounter(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<(), String> {
    if let Some(party_id) = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .and_then(|character| character.party_id)
    {
        require_no_unresolved_encounter(ctx, &party_id)?;
    }
    Ok(())
}
