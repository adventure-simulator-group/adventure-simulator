#[reducer]
pub fn travel_to_case_site(
    ctx: &ReducerContext,
    character_id: u64,
    case_site_id: CaseSiteId,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    case_site_id
        .to_place()
        .ok_or("Case-site identity is malformed")?;
    travel_to_case_site_impl(ctx, character_id, case_site_id.value, None)
}

#[reducer]
pub fn travel_to_case_site_planned(
    ctx: &ReducerContext,
    character_id: u64,
    case_site_id: CaseSiteId,
    route: JourneyRoutePlan,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    case_site_id
        .to_place()
        .ok_or("Case-site identity is malformed")?;
    travel_to_case_site_impl(ctx, character_id, case_site_id.value, Some(route))
}

fn authoritative_straight_line_case_route(
    departure_minute: u64,
    origin: (f64, f64),
    destination: (f64, f64),
    coordinates_are_geographic: bool,
    distance_m: u64,
    minutes: u64,
) -> Result<JourneyRoutePlan, String> {
    let origin = encode_position_e7(origin.0, origin.1, coordinates_are_geographic)
        .ok_or("Journey origin is not a valid WGS84 coordinate")?;
    let destination = encode_position_e7(destination.0, destination.1, coordinates_are_geographic)
        .ok_or("Journey destination is not a valid WGS84 coordinate")?;
    let points = vec![
        JourneyRoutePoint {
            latitude_e7: origin.latitude_e7,
            longitude_e7: origin.longitude_e7,
        },
        JourneyRoutePoint {
            latitude_e7: destination.latitude_e7,
            longitude_e7: destination.longitude_e7,
        },
    ];
    let origin_microdegrees = adventuresim_world_schema::coordinates::Wgs84CoordinateE7::new(
        points[0].latitude_e7,
        points[0].longitude_e7,
    )
    .map(adventuresim_world_schema::coordinates::Wgs84CoordinateMicrodegrees::from_e7)
    .ok_or("Journey origin is not a valid WGS84 coordinate")?;
    let weather = adventuresim_core::weather::weather_at(
        adventuresim_core::weather::WORLD_WEATHER_SEED,
        departure_minute,
        origin_microdegrees.latitude().get(),
        origin_microdegrees.longitude().get(),
        0,
    );
    let precipitation = match weather.precipitation {
        adventuresim_core::weather::Precipitation::Clear => JourneyPrecipitation::Clear,
        adventuresim_core::weather::Precipitation::Rain => JourneyPrecipitation::Rain,
        adventuresim_core::weather::Precipitation::Snow => JourneyPrecipitation::Snow,
    };
    let digest_domain = format!(
        "authoritative-straight-line-v1:{departure_minute}:{:?}:{:?}:{distance_m}:{minutes}",
        points[0], points[1]
    );
    let package_digest = format!(
        "{:016x}{:016x}{:016x}{:016x}",
        adventuresim_core::settlement_population::stable_hash(&digest_domain),
        adventuresim_core::settlement_population::stable_hash(&(digest_domain.clone() + ":1")),
        adventuresim_core::settlement_population::stable_hash(&(digest_domain.clone() + ":2")),
        adventuresim_core::settlement_population::stable_hash(&(digest_domain + ":3")),
    );
    let span = JourneyTerrainSpan {
        kind: JourneyTerrainKind::Open,
        terrain: JourneyTerrainWeights {
            plains: 1_000,
            forest: 0,
            hills: 0,
            wetlands: 0,
            urban: 0,
        },
        training_multiplier_permille: 1_000,
        check_millirank: 0,
        start_minute: 0,
        duration_minutes: minutes,
    };
    Ok(JourneyRoutePlan {
        package_digest,
        weather_rules_version: weather.rules_version,
        weather_interval_start: weather.interval_start_minute,
        precipitation,
        intensity_bps: weather.intensity_bps,
        ground_moisture_bps: weather.ground_moisture_bps,
        snow_cover_bps: weather.snow_cover_bps,
        distance_m,
        minutes,
        points: points.clone(),
        spans: vec![span.clone()],
        return_route: Some(JourneyRouteLeg {
            distance_m,
            minutes,
            points: points.into_iter().rev().collect(),
            spans: vec![span],
        }),
    })
}

fn travel_to_case_site_impl(
    ctx: &ReducerContext,
    character_id: u64,
    case_site_id: String,
    route: Option<JourneyRoutePlan>,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    let Some(party_id) = character.party_id.clone() else {
        return Err("Must be in a party to travel to a case site".into());
    };
    let Some(mut party) = ctx.db.party_authority().id().find(&party_id) else {
        return Err("Party not found".into());
    };
    if party.leader_id != character_id {
        return Err("Only the party leader can travel".into());
    }
    require_no_unresolved_encounter(ctx, &party_id)?;
    if party.camp_destination.is_some() {
        return Err("Break camp and continue the current journey first".into());
    }
    exact_case_site_for_observer(ctx, character_id, &case_site_id)
        .ok_or("That exact site has not been disclosed to this observer")?;
    let expected_settlement_id = party.current_settlement_id.clone();
    let expected_case_site_id = party.current_case_site_id.clone();
    if expected_settlement_id.is_some() == expected_case_site_id.is_some() {
        return Err("Party must be at one authoritative location to travel".into());
    }
    if character.current_settlement_id != expected_settlement_id
        || crate::investigation::character_case_site_id(ctx, character_id)
            != expected_case_site_id.as_ref().map(|id| id.value.clone())
    {
        return Err("Party leader location does not match the party".into());
    }
    if expected_case_site_id
        .as_ref()
        .is_some_and(|origin| origin.value == case_site_id)
    {
        return Err("The party is already at that case site".into());
    }
    require_party_ready(ctx, &party_id)?;
    let traveler_ids = living_party_member_ids(ctx, &party_id);
    let Some(departure_minute) = crate::time::synchronize_party_departure_time(ctx, &traveler_ids)?
    else {
        return Ok(());
    };
    party = revalidate_party_after_departure_sync(
        ctx,
        &party_id,
        character_id,
        expected_settlement_id.as_deref(),
        expected_case_site_id.as_deref(),
        None,
        false,
    )?;
    let (site, lead) = exact_case_site_for_observer(ctx, character_id, &case_site_id)
        .ok_or("Exact destination knowledge changed during departure synchronization")?;
    let traveler_ids = living_party_member_ids(ctx, &party_id);

    let (origin_endpoint, origin_coordinates, origin_is_geographic, departing_settlement) =
        if let Some(origin_id) = expected_settlement_id.as_deref() {
            let origin = ctx
                .db
                .settlement()
                .id()
                .find(origin_id.to_owned())
                .ok_or("Current settlement not found")?;
            let origin_is_geographic =
                site.coordinates_are_geographic && origin.source_node_id.is_some();
            let origin_coordinates = if origin_is_geographic {
                Wgs84CoordinateE7::from_longitude_latitude_degrees(origin.coord_x, origin.coord_y)
                    .ok_or("Current settlement has an invalid WGS84 coordinate")?
                    .longitude_latitude_degrees()
            } else {
                (origin.coord_x, origin.coord_y)
            };
            (
                JourneyEndpoint::Settlement(JourneySettlementEndpoint {
                    id: origin.id.clone(),
                    name: origin.name,
                }),
                origin_coordinates,
                origin_is_geographic,
                true,
            )
        } else {
            let origin_id = expected_case_site_id
                .as_ref()
                .ok_or("Current case site not found")?;
            let origin = ctx
                .db
                .case_site_authority()
                .id_key()
                .find(&origin_id.value)
                .ok_or("Current case site not found")?;
            let origin_is_geographic =
                site.coordinates_are_geographic && origin.coordinates_are_geographic;
            let origin_coordinates = decode_position_e7(
                origin.longitude_e7,
                origin.latitude_e7,
                origin_is_geographic,
            )
            .ok_or("Current case site has an invalid WGS84 coordinate")?;
            (
                JourneyEndpoint::CaseSite(JourneyCaseSiteEndpoint {
                    id: origin.id.clone(),
                    name: origin.name,
                }),
                origin_coordinates,
                origin_is_geographic,
                false,
            )
        };
    let destination = decode_position_e7(lead.longitude_e7, lead.latitude_e7, origin_is_geographic)
        .ok_or("Destination case site has an invalid WGS84 coordinate")?;
    if let Some(route) = route.as_ref() {
        validate_route_departure_weather_interval(route, departure_minute)?;
        validate_journey_route(ctx, route, origin_coordinates, destination)?;
        validate_return_journey_route(ctx, route, destination, origin_coordinates)?;
    }
    let distance_m = straight_line_distance_m(
        origin_coordinates.0,
        origin_coordinates.1,
        destination.0,
        destination.1,
        origin_is_geographic,
    );
    let travel_minutes = route
        .as_ref()
        .map_or_else(|| quest_journey_minutes(distance_m), |route| route.minutes);
    let route = match route {
        Some(route) => route,
        None => authoritative_straight_line_case_route(
            departure_minute,
            origin_coordinates,
            destination,
            origin_is_geographic,
            distance_m,
            travel_minutes,
        )?,
    };
    start_party_journey(
        ctx,
        &party,
        origin_endpoint,
        JourneyEndpoint::CaseSite(JourneyCaseSiteEndpoint {
            id: CaseSiteId::try_new(site.id.value.clone())?,
            name: site.name.clone(),
        }),
        travel_minutes,
        departure_minute,
        Some(&route),
    )?;
    crate::condition::prepare_party_waterskins(ctx, &party_id, departing_settlement)?;
    for member_id in traveler_ids.iter().copied() {
        crate::condition::prepare_character_waterskins(ctx, member_id, departing_settlement)?;
    }
    let proposed_leg_minutes =
        travel_minutes.min(party_next_walking_minutes(ctx, &party.id, travel_minutes)?);
    let (leg_minutes, encounter, narrative, next_roll) = advance_party_movement_until_encounter(
        ctx,
        &party_id,
        &traveler_ids,
        proposed_leg_minutes,
    )?;
    party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party changed during travel")?;
    let interrupted = encounter.is_some() || narrative.is_some();
    if interrupted || leg_minutes < travel_minutes {
        for member_id in living_party_member_ids(ctx, &party_id) {
            let mut member = ctx
                .db
                .character()
                .id()
                .find(member_id)
                .ok_or("Party member not found")?;
            member.current_settlement_id = None;
            crate::investigation::set_character_case_site(ctx, member.id, None)?;
            ctx.db.character().id().update(member);
        }
        set_party_journey_state(
            &mut party,
            None,
            None,
            Some(JourneyEndpoint::CaseSite(JourneyCaseSiteEndpoint {
                id: CaseSiteId::from(case_site_id),
                name: site.name.clone(),
            })),
            travel_minutes.saturating_sub(leg_minutes),
        );
        ctx.db.party_authority().id().update(party);
        if interrupted {
            record_party_journey_interruption(ctx, &party_id, leg_minutes);
            commit_encounter_scan(ctx, &party_id, next_roll, encounter, narrative)?;
        } else {
            // A departure outside the walking window reaches a real initial
            // camp at movement minute zero. Persist that reached identity just
            // like every later camp so rest/continue and fixture custody agree.
            record_party_journey_camp(ctx, &party_id, leg_minutes)?;
            commit_encounter_scan(ctx, &party_id, next_roll, None, None)?;
        }
        return Ok(());
    }
    for member_id in traveler_ids {
        if let Some(mut member) = ctx.db.character().id().find(member_id) {
            member.current_settlement_id = None;
            crate::investigation::set_character_case_site(
                ctx,
                member.id,
                Some(case_site_id.clone()),
            )?;
            ctx.db.character().id().update(member);
            mark_case_site_visited(ctx, member_id, &site)?;
        }
    }
    set_party_journey_state(
        &mut party,
        None,
        Some(CaseSiteId::from(case_site_id)),
        None,
        0,
    );
    ctx.db.party_authority().id().update(party);
    commit_case_site_arrival_objectives(ctx, &party_id, &site)?;
    finish_party_journey(ctx, &party_id);
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "arrival applies each distinct travel, party, and companion outcome"
)]
fn complete_settlement_arrival(
    ctx: &ReducerContext,
    traveler_ids: Vec<u64>,
    mut party: Option<&mut Party>,
    destination: &Settlement,
    settlement_id: &str,
    departing_case_site: Option<&str>,
    travel_minutes_to_advance: Option<u64>,
    rest_temporary_companions: bool,
) -> Result<(), String> {
    let canonical_excursion = party
        .as_ref()
        .and_then(|party| party.wilderness_canonical_anchor_minute)
        .map(|start| crate::time::refresh_clock(ctx).map(|end| (start, end)))
        .transpose()?;
    for traveler_id in traveler_ids {
        if let Some((canonical_start, canonical_end)) = canonical_excursion {
            crate::condition::apply_canonical_wilderness_observance(
                ctx,
                traveler_id,
                canonical_start,
                canonical_end,
            )?;
        }
        if let Some(travel_minutes) = travel_minutes_to_advance
            && !advance_travel_time(ctx, traveler_id, travel_minutes)?
        {
            return Ok(());
        }
        let mut traveler = ctx
            .db
            .character()
            .id()
            .find(traveler_id)
            .ok_or("Party member not found")?;
        traveler.current_settlement_id = Some(settlement_id.to_owned());
        crate::investigation::set_character_case_site(ctx, traveler.id, None)?;
        ctx.db.character().id().update(traveler);
        if !crate::time::synchronize_to_settlement_time_of_day(ctx, traveler_id)? {
            continue;
        }
        crate::condition::replenish_needs_at_settlement(ctx, traveler_id)?;
        crate::condition::refresh_character_strategic_condition(ctx, traveler_id)?;
        crate::organization::reconcile_presentation(ctx, traveler_id)?;
        crate::capability::refresh_character_capability(ctx, traveler_id)?;
        if rest_temporary_companions {
            crate::time::rest_temporary_party_member_until_healed_at_settlement(ctx, traveler_id)?;
        }
    }

    if let Some(party) = party.as_mut() {
        set_party_journey_state(party, Some(settlement_id.to_owned()), None, None, 0);
        ctx.db.party_authority().id().update((*party).clone());
        finish_party_journey(ctx, &party.id);
        let departing_incident = departing_case_site.and_then(|site_id| {
            ctx.db.strategic_incident().iter().find(|incident| {
                incident.party_id == party.id
                    && incident.case_site_id.value == site_id
                    && incident.status == IncidentStatus::Pending
            })
        });
        if let Some(incident) = departing_incident.as_ref() {
            if incident.kind == IncidentKind::AuthorityArrest {
                let minute = crate::time::refresh_clock(ctx)?;
                crate::reputation::record_event(
                    ctx,
                    format!("avoid-authority:{}", incident.id.value),
                    incident.instigator_id,
                    &incident.settlement_id,
                    "avoiding_authority",
                    &incident.id.value,
                    0,
                    300,
                    minute,
                )?;
                crate::reputation::record_discovered_offense(
                    ctx,
                    format!("offense:avoid-authority:{}", incident.id.value),
                    incident.instigator_id,
                    &incident.settlement_id,
                    "avoiding_authority",
                    2,
                    minute,
                );
            }
            finish_strategic_incident(ctx, &incident.id, IncidentStatus::Avoided)?;
        }
        if departing_incident.is_none() {
            let religious = maybe_trigger_religious_incident(ctx, &party.id, destination)?;
            if religious.is_none() {
                maybe_trigger_activity_incident(
                    ctx,
                    party.leader_id,
                    crate::time::ActivityRisks::default(),
                )?;
            }
        }
    }
    Ok(())
}

#[reducer]
pub fn travel_to_settlement(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    require_strategic_character_authority(ctx, character_id)?;
    travel_to_settlement_impl(ctx, character_id, settlement_id, None)
}

#[reducer]
pub fn travel_to_settlement_planned(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    route: JourneyRoutePlan,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    require_strategic_character_authority(ctx, character_id)?;
    travel_to_settlement_impl(ctx, character_id, settlement_id, Some(route))
}

fn travel_to_settlement_impl(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    route: Option<JourneyRoutePlan>,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
    let Some(destination) = ctx.db.settlement().id().find(&settlement_id) else {
        return Err("Settlement not found".into());
    };

    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    let mut party = character
        .party_id
        .as_ref()
        .map(|party_id| {
            ctx.db
                .party_authority()
                .id()
                .find(party_id)
                .ok_or_else(|| "Party not found".to_string())
        })
        .transpose()?;
    if let Some(party) = party.as_ref() {
        if party.leader_id != character_id
            && !ready_companion_may_start_evacuation(ctx, party, character_id, &settlement_id)
        {
            return Err(
                "Only the party leader, or a ready companion evacuating an unready leader, can travel"
                    .into(),
            );
        }
        require_no_unresolved_encounter(ctx, &party.id)?;
    }

    // Choosing a different camp destination only changes the planned route.
    // The party can rest before it attempts the newly selected leg.
    if let Some(party) = party.as_mut()
        && party.camp_destination.is_some()
    {
        return redirect_camped_party_to_settlement(ctx, party, &destination, route);
    }

    if let Some(party) = party.as_ref() {
        // A defeated party can withdraw from an off-road quest location to
        // recover at a settlement, but may not begin ordinary travel while a
        // member is incapacitated.
        if party.current_case_site_id.is_none() {
            require_party_ready(ctx, &party.id)?;
        }
    } else {
        crate::condition::require_character_ready(ctx, character_id)?;
    }

    let (travel_minutes, origin_kind, origin_id, origin_name, zero_distance_case_site_return) =
        if let Some(origin_id) = &character.current_settlement_id {
            let Some(origin) = ctx.db.settlement().id().find(origin_id) else {
                return Err("Character's current settlement does not exist".into());
            };
            // Demo settlements remain usable before a Viabundus world is loaded.
            // Imported journeys must lead to the next settlement on the road graph.
            let minutes = if let (Some(origin_node), Some(destination_node)) =
                (origin.source_node_id, destination.source_node_id)
            {
                let Some(distance_m) = connected_settlement_distances(ctx, origin_node)
                    .get(&destination_node)
                    .copied()
                else {
                    return Err("That settlement is not directly connected by land or ferry".into());
                };
                journey_minutes(distance_m)
            } else {
                let distance_km = ((origin.coord_x - destination.coord_x).powi(2)
                    + (origin.coord_y - destination.coord_y).powi(2))
                .sqrt()
                .ceil() as u64;
                journey_minutes(distance_km.saturating_mul(METERS_PER_KILOMETER))
            };
            if let Some(route) = route.as_ref() {
                validate_journey_route(
                    ctx,
                    route,
                    (origin.coord_x, origin.coord_y),
                    (destination.coord_x, destination.coord_y),
                )?;
            }
            let minutes = route.as_ref().map_or(minutes, |route| route.minutes);
            (minutes, "settlement", origin.id, origin.name, false)
        } else if let Some(case_site_id) =
            crate::investigation::character_case_site_id(ctx, character_id)
        {
            let Some(site) = ctx.db.case_site_authority().id_key().find(case_site_id) else {
                return Err("Character's current case site does not exist".into());
            };
            let coordinates_are_geographic =
                site.coordinates_are_geographic && destination.source_node_id.is_some();
            let (site_x, site_y) = decode_position_e7(
                site.longitude_e7,
                site.latitude_e7,
                coordinates_are_geographic,
            )
            .ok_or("Current case site has an invalid WGS84 coordinate")?;
            let distance_m = straight_line_distance_m(
                site_x,
                site_y,
                destination.coord_x,
                destination.coord_y,
                coordinates_are_geographic,
            );
            let zero_distance_return = distance_m == 0;
            if !zero_distance_return && let Some(route) = route.as_ref() {
                validate_journey_route(
                    ctx,
                    route,
                    (site_x, site_y),
                    (destination.coord_x, destination.coord_y),
                )?;
            }
            (
                if zero_distance_return {
                    0
                } else {
                    route
                        .as_ref()
                        .map_or_else(|| quest_journey_minutes(distance_m), |route| route.minutes)
                },
                "case_site",
                site.id.value,
                site.name,
                zero_distance_return,
            )
        } else {
            return Err("Character is not at a known location".into());
        };

    let departing_case_site = crate::investigation::character_case_site_id(ctx, character_id);
    let traveler_ids: Vec<u64> = if let Some(party) = party.as_ref() {
        living_party_member_ids(ctx, &party.id)
    } else {
        vec![character_id]
    };
    if zero_distance_case_site_return {
        crate::foraging::current_strategic_place(ctx, character_id)?;
        return complete_settlement_arrival(
            ctx,
            traveler_ids,
            party.as_mut(),
            &destination,
            &settlement_id,
            departing_case_site.as_deref(),
            None,
            false,
        );
    }
    let Some(departure_minute) = crate::time::synchronize_party_departure_time(ctx, &traveler_ids)?
    else {
        return Ok(());
    };
    if let Some(route) = route.as_ref() {
        validate_route_departure_weather_interval(route, departure_minute)?;
    }
    if let Some(current_party) = party.as_ref() {
        let expected_leader_id = current_party.leader_id;
        party = Some(revalidate_party_after_departure_sync(
            ctx,
            &current_party.id,
            expected_leader_id,
            (origin_kind == "settlement").then_some(origin_id.as_str()),
            (origin_kind == "case_site").then_some(origin_id.as_str()),
            None,
            origin_kind == "case_site",
        )?);
    }
    let traveler_ids: Vec<u64> = if let Some(party) = party.as_ref() {
        living_party_member_ids(ctx, &party.id)
    } else {
        vec![character_id]
    };
    if let Some(party) = party.as_ref() {
        start_party_journey(
            ctx,
            party,
            match origin_kind {
                "settlement" => JourneyEndpoint::Settlement(JourneySettlementEndpoint {
                    id: origin_id.clone(),
                    name: origin_name.clone(),
                }),
                "case_site" => JourneyEndpoint::CaseSite(JourneyCaseSiteEndpoint {
                    id: CaseSiteId::try_new(origin_id.clone())?,
                    name: origin_name.clone(),
                }),
                _ => return Err("Journey origin kind is invalid".into()),
            },
            JourneyEndpoint::Settlement(JourneySettlementEndpoint {
                id: destination.id.clone(),
                name: destination.name.clone(),
            }),
            travel_minutes,
            departure_minute,
            route.as_ref(),
        )?;
    }
    let departing_settlement = character.current_settlement_id.is_some();
    if let Some(current_party) = party.as_ref() {
        crate::condition::prepare_party_waterskins(ctx, &current_party.id, departing_settlement)?;
    }
    for traveler_id in traveler_ids.iter().copied() {
        crate::condition::prepare_character_waterskins(ctx, traveler_id, departing_settlement)?;
    }
    let mut party_movement_committed = false;
    if let Some(current_party) = party.as_ref() {
        let party_id = current_party.id.clone();
        let proposed_leg_minutes =
            travel_minutes.min(party_next_walking_minutes(ctx, &party_id, travel_minutes)?);
        let (leg_minutes, encounter, narrative, next_roll) =
            advance_party_movement_until_encounter(
                ctx,
                &party_id,
                &traveler_ids,
                proposed_leg_minutes,
            )?;
        party = Some(
            ctx.db
                .party_authority()
                .id()
                .find(&party_id)
                .ok_or("Party changed during travel")?,
        );
        party_movement_committed = true;
        let interrupted = encounter.is_some() || narrative.is_some();
        if interrupted || leg_minutes < travel_minutes {
            for traveler_id in living_party_member_ids(ctx, &party_id) {
                let mut traveler = ctx
                    .db
                    .character()
                    .id()
                    .find(traveler_id)
                    .ok_or("Party member not found")?;
                traveler.current_settlement_id = None;
                crate::investigation::set_character_case_site(ctx, traveler.id, None)?;
                ctx.db.character().id().update(traveler);
            }
            let party = party.as_mut().expect("party was just reloaded");
            set_party_journey_state(
                party,
                None,
                None,
                Some(JourneyEndpoint::Settlement(JourneySettlementEndpoint {
                    id: settlement_id,
                    name: destination.name.clone(),
                })),
                travel_minutes.saturating_sub(leg_minutes),
            );
            ctx.db.party_authority().id().update(party.clone());
            if interrupted {
                record_party_journey_interruption(ctx, &party.id, leg_minutes);
                commit_encounter_scan(ctx, &party.id, next_roll, encounter, narrative)?;
            } else {
                record_party_journey_camp(ctx, &party.id, leg_minutes)?;
                commit_encounter_scan(ctx, &party.id, next_roll, None, None)?;
            }
            return Ok(());
        }
    }
    complete_settlement_arrival(
        ctx,
        traveler_ids,
        party.as_mut(),
        &destination,
        &settlement_id,
        departing_case_site.as_deref(),
        (!party_movement_committed).then_some(travel_minutes),
        true,
    )
}

#[reducer]
pub fn set_party_camp_fatigue_percent(
    ctx: &ReducerContext,
    character_id: u64,
    fatigue_percent: u8,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
    if !(10..=100).contains(&fatigue_percent) {
        return Err("Camp fatigue must be between 10% and 100%".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Character is not in a party")?;
    let mut party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can configure travel".into());
    }
    party.camp_fatigue_percent = fatigue_percent;
    ctx.db.party_authority().id().update(party);
    Ok(())
}

#[reducer]
pub fn set_party_travel_itinerary(
    ctx: &ReducerContext,
    character_id: u64,
    walking_minutes_per_day: u16,
    travel_at_night: bool,
    journey_start_minute_of_day: u16,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
    if walking_minutes_per_day > adventuresim_core::strategic_time::MAX_WALKING_MINUTES_PER_DAY
        || (walking_minutes_per_day > 0
            && daylight_walking_window(walking_minutes_per_day).is_none())
    {
        return Err("Daily walking time must be between 0 and 24 hours".into());
    }
    let Some(journey_start) =
        adventuresim_core::strategic_time::StrategicMinuteOfDay::new(
            journey_start_minute_of_day,
        )
    else {
        return Err("Journey departure time must be within one day".into());
    };
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Character is not in a party")?;
    let mut party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can configure travel".into());
    }
    if party.wilderness_canonical_anchor_minute.is_some()
        && party.journey_start_minute_of_day != journey_start.get()
    {
        return Err("Journey departure time cannot change after setting out".into());
    }
    party.walking_minutes_per_day = walking_minutes_per_day;
    party.travel_at_night = travel_at_night;
    party.journey_start_minute_of_day = journey_start.get();
    let camped = party.camp_destination.is_some();
    ctx.db.party_authority().id().update(party);
    if camped {
        refresh_party_journey_forecast(ctx, &party_id)?;
    }
    Ok(())
}

/// Advance a single planned leg from a camp. A journey remains a strategic
/// state, rather than a tactical simulation: the UI animates this instantaneous
/// transition between pins.
#[reducer]
pub fn continue_camp_travel(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    crate::character::require_living_character(ctx, character_id)?;
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Character is not in a party")?;
    let mut party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if !party_can_continue_travel(&party, character_id)
        && !ready_companion_may_continue_evacuation(ctx, &party, character_id)
    {
        return Err(
            "Only the party leader, or a ready companion evacuating an unready leader, can continue travel"
                .into(),
        );
    }
    require_no_unresolved_encounter(ctx, &party_id)?;
    let destination = party
        .camp_destination
        .clone()
        .ok_or("The party is not camped")?;
    let camp_place = current_journey_camp_place(ctx, &party_id)?;
    crate::food::require_clear_current_camp_fireplace(ctx, &camp_place)?;
    // Refresh only after the exact pre-refresh camp and every persisted
    // fireplace custody row have been validated. Forecast refresh must never
    // mint a new identity around existing custody.
    refresh_party_journey_forecast(ctx, &party_id)?;
    let proposed_leg_minutes = party.camp_remaining_minutes.min(party_next_walking_minutes(
        ctx,
        &party.id,
        party.camp_remaining_minutes,
    )?);
    if proposed_leg_minutes == 0 {
        return Err(adventuresim_core::reducer_error::coded_reducer_error(
            adventuresim_core::reducer_error::ReducerErrorCode::JourneyDaylightWindowRequired,
            "Rest until the party reaches its next daylight walking window",
        ));
    }
    let traveler_ids = living_party_member_ids(ctx, &party_id);
    let (leg_minutes, encounter, narrative, next_roll) = advance_party_movement_until_encounter(
        ctx,
        &party_id,
        &traveler_ids,
        proposed_leg_minutes,
    )?;
    party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party changed during travel")?;
    let interrupted = encounter.is_some() || narrative.is_some();
    party.camp_remaining_minutes = party.camp_remaining_minutes.saturating_sub(leg_minutes);
    if interrupted || party.camp_remaining_minutes > 0 {
        ctx.db.party_authority().id().update(party);
        if interrupted {
            record_party_journey_interruption(ctx, &party_id, leg_minutes);
            commit_encounter_scan(ctx, &party_id, next_roll, encounter, narrative)?;
        } else {
            record_party_journey_camp(ctx, &party_id, leg_minutes)?;
            commit_encounter_scan(ctx, &party_id, next_roll, None, None)?;
        }
        return Ok(());
    }
    match destination {
        JourneyEndpoint::Settlement(endpoint) => {
            let destination_id = endpoint.id;
            let _destination = ctx
                .db
                .settlement()
                .id()
                .find(&destination_id)
                .ok_or("Camp destination settlement not found")?;
            for member_id in traveler_ids.iter().copied() {
                let mut member = ctx
                    .db
                    .character()
                    .id()
                    .find(member_id)
                    .ok_or("Party member not found")?;
                member.current_settlement_id = Some(destination_id.clone());
                crate::investigation::set_character_case_site(ctx, member.id, None)?;
                ctx.db.character().id().update(member);
                crate::condition::replenish_needs_at_settlement(ctx, member_id)?;
                crate::condition::refresh_character_strategic_condition(ctx, member_id)?;
                crate::organization::reconcile_presentation(ctx, member_id)?;
                crate::time::rest_temporary_party_member_until_healed_at_settlement(
                    ctx, member_id,
                )?;
            }
            party.current_settlement_id = Some(destination_id);
            party.current_case_site_id = None;
        }
        JourneyEndpoint::CaseSite(endpoint) => {
            let destination_id = endpoint.id.value;
            let site = ctx
                .db
                .case_site_authority()
                .id_key()
                .find(&destination_id)
                .ok_or("Camp destination case site not found")?;
            for member_id in traveler_ids.iter().copied() {
                let mut member = ctx
                    .db
                    .character()
                    .id()
                    .find(member_id)
                    .ok_or("Party member not found")?;
                member.current_settlement_id = None;
                crate::investigation::set_character_case_site(
                    ctx,
                    member.id,
                    Some(destination_id.clone()),
                )?;
                ctx.db.character().id().update(member);
                mark_case_site_visited(ctx, member_id, &site)?;
                crate::condition::refresh_character_strategic_condition(ctx, member_id)?;
            }
            party.current_settlement_id = None;
            party.current_case_site_id = Some(CaseSiteId::from(destination_id));
        }
        JourneyEndpoint::Camp(_) => return Err("A camp cannot be a journey destination".into()),
    }
    let current_settlement_id = party.current_settlement_id.clone();
    let current_case_site_id = party.current_case_site_id.clone();
    set_party_journey_state(
        &mut party,
        current_settlement_id,
        current_case_site_id,
        None,
        0,
    );
    ctx.db.party_authority().id().update(party);
    if let Some(arrived_site) = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .and_then(|party| party.current_case_site_id)
        .and_then(|site_id| ctx.db.case_site_authority().id_key().find(&site_id.value))
    {
        commit_case_site_arrival_objectives(ctx, &party_id, &arrived_site)?;
    }
    finish_party_journey(ctx, &party_id);
    Ok(())
}
