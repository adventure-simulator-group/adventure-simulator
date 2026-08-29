//! Public weather, thermal, and itinerary safety projections.

use super::*;

/// Keep the public minute-by-minute weather projection bounded. A route beyond
/// sixty days is not a credible case-site leg and fails closed instead of
/// consuming unbounded simulator work.
pub(super) const MAX_CASE_SITE_THERMAL_FORECAST_MINUTES: u64 = 60 * MINUTES_PER_DAY;
pub(super) fn public_straight_line_distance_m(
    origin: PublicRoutePoint,
    destination: PublicRoutePoint,
    geographic: bool,
) -> u64 {
    let longitude_delta = (i64::from(destination.longitude.get())
        - i64::from(origin.longitude.get())) as f64
        / f64::from(LongitudeMicrodegrees::UNITS_PER_DEGREE);
    let latitude_delta = (i64::from(destination.latitude.get()) - i64::from(origin.latitude.get()))
        as f64
        / f64::from(LatitudeMicrodegrees::UNITS_PER_DEGREE);
    let distance_m = if geographic {
        let origin_latitude = origin.latitude.degrees();
        let destination_latitude = destination.latitude.degrees();
        let lat1 = origin_latitude.to_radians();
        let lat2 = destination_latitude.to_radians();
        let delta_lat = latitude_delta.to_radians();
        let delta_lon = longitude_delta.to_radians();
        let a = ((delta_lat / 2.0).sin().powi(2)
            + lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2))
        .clamp(0.0, 1.0);
        6_371_000.0 * 2.0 * a.sqrt().atan2((1.0 - a).sqrt())
    } else {
        longitude_delta.hypot(latitude_delta) * 1_000.0
    };
    distance_m.round().max(1.0) as u64
}

pub(super) fn case_site_movement_minutes(distance_m: u64) -> Option<u64> {
    (distance_m > 0).then(|| ((distance_m as f64 / 1_250.0) * 60.0).ceil() as u64)
}

pub(super) fn projected_itinerary_thermal_safe(
    starting_minute: u64,
    itinerary: &adventuresim_core::strategic_time::ItineraryForecast,
    origin: PublicRoutePoint,
    destination: PublicRoutePoint,
    starting_state: adventuresim_core::survival::SurvivalState,
    insulation_bps: u16,
    has_tent: bool,
) -> Option<bool> {
    projected_itinerary_thermal_state(
        starting_minute,
        itinerary,
        origin,
        destination,
        starting_state,
        insulation_bps,
        has_tent,
    )
    .map(|projection| projection.safe)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PublicThermalProjection {
    pub(super) state: adventuresim_core::survival::SurvivalState,
    pub(super) safe: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PublicRoundTripRoute {
    pub(super) origin: PublicRoutePoint,
    pub(super) destination: PublicRoutePoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PublicThermalTraveler {
    pub(super) starting_state: adventuresim_core::survival::SurvivalState,
    pub(super) insulation_bps: u16,
    pub(super) has_tent: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RoundTripThermalProjection<'a> {
    pub(super) starting_minute: u64,
    pub(super) outbound_itinerary: &'a adventuresim_core::strategic_time::ItineraryForecast,
    pub(super) return_itinerary: &'a adventuresim_core::strategic_time::ItineraryForecast,
    pub(super) action_minutes: u64,
    pub(super) route: PublicRoundTripRoute,
    pub(super) traveler: PublicThermalTraveler,
}

pub(super) fn projected_itinerary_thermal_state(
    starting_minute: u64,
    itinerary: &adventuresim_core::strategic_time::ItineraryForecast,
    origin: PublicRoutePoint,
    destination: PublicRoutePoint,
    starting_state: adventuresim_core::survival::SurvivalState,
    insulation_bps: u16,
    has_tent: bool,
) -> Option<PublicThermalProjection> {
    if itinerary.truncated
        || itinerary.total_elapsed_minutes == 0
        || itinerary.total_elapsed_minutes > MAX_CASE_SITE_THERMAL_FORECAST_MINUTES
        || itinerary.total_movement_minutes == 0
    {
        return None;
    }
    let clothing = adventuresim_core::survival::ClothingExposure {
        insulation_bps,
        // Public equipped definitions are sufficient to reproduce insulation.
        // Layer ordering is not projected here, so rain protection deliberately
        // fails safe at zero rather than assuming an advantageous outer shell.
        weatherproofing_bps: 0,
        peripheral_protection_bps: [0; 4],
    };
    let mut state = starting_state;
    for segment in &itinerary.segments {
        for local_offset in 0..segment.elapsed_minutes {
            let offset = segment.elapsed_start.saturating_add(local_offset);
            let movement_offset = segment.movement_start.saturating_add(
                if segment.kind == adventuresim_core::strategic_time::ItinerarySegmentKind::Walking
                {
                    local_offset.min(segment.movement_minutes)
                } else {
                    0
                },
            );
            let interpolate = |start: i32, end: i32| {
                let delta = i64::from(end) - i64::from(start);
                (i64::from(start)
                    + delta.saturating_mul(movement_offset as i64)
                        / itinerary.total_movement_minutes as i64)
                    .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
            };
            let elevation = interpolate(
                i32::from(origin.elevation_m),
                i32::from(destination.elevation_m),
            )
            .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
            let weather = adventuresim_core::weather::weather_at(
                adventuresim_core::weather::WORLD_WEATHER_SEED,
                starting_minute.saturating_add(offset),
                interpolate(origin.latitude.get(), destination.latitude.get()),
                interpolate(origin.longitude.get(), destination.longitude.get()),
                elevation,
            );
            let shelter = if segment.kind
                == adventuresim_core::strategic_time::ItinerarySegmentKind::Camp
                && has_tent
            {
                adventuresim_core::survival::ExposureShelter::Field(
                    adventuresim_core::survival::FieldShelter::Tent,
                )
            } else {
                adventuresim_core::survival::ExposureShelter::Field(
                    adventuresim_core::survival::FieldShelter::Bivouac,
                )
            };
            state = adventuresim_core::survival::advance_exposure(
                state,
                std::iter::once(weather),
                clothing,
                shelter,
            )
            .state;
            if state.thermal_strain <= adventuresim_core::survival::COLD_STAGGER_STRAIN
                || state.thermal_strain >= adventuresim_core::survival::HEAT_STAGGER_STRAIN
            {
                return Some(PublicThermalProjection { state, safe: false });
            }
        }
    }
    Some(PublicThermalProjection { state, safe: true })
}

pub(super) fn projected_stationary_outdoor_thermal_state(
    starting_minute: u64,
    duration_minutes: u64,
    location: PublicRoutePoint,
    starting_state: adventuresim_core::survival::SurvivalState,
    insulation_bps: u16,
) -> Option<PublicThermalProjection> {
    projected_stationary_field_thermal_state(
        starting_minute,
        duration_minutes,
        location,
        starting_state,
        insulation_bps,
        false,
    )
}

pub(super) fn projected_stationary_field_thermal_state(
    starting_minute: u64,
    duration_minutes: u64,
    location: PublicRoutePoint,
    starting_state: adventuresim_core::survival::SurvivalState,
    insulation_bps: u16,
    has_tent: bool,
) -> Option<PublicThermalProjection> {
    if duration_minutes == 0 {
        return Some(PublicThermalProjection {
            state: starting_state,
            safe: true,
        });
    }
    let itinerary = adventuresim_core::strategic_time::ItineraryForecast {
        segments: vec![adventuresim_core::strategic_time::ItinerarySegment {
            kind: adventuresim_core::strategic_time::ItinerarySegmentKind::Camp,
            elapsed_start: 0,
            elapsed_minutes: duration_minutes,
            movement_start: 0,
            movement_minutes: 0,
            average_fatigue_start: 0.0,
            average_fatigue_end: 0.0,
            maximum_fatigue_end: 0.0,
            required_rest_minutes: 0,
        }],
        member_final_fatigue: vec![0.0],
        member_maximum_fatigue: vec![0.0],
        total_elapsed_minutes: duration_minutes,
        // The shared itinerary projector requires a nonzero movement bound;
        // identical endpoints keep this stationary despite that sentinel.
        total_movement_minutes: 1,
        truncated: false,
    };
    projected_itinerary_thermal_state(
        starting_minute,
        &itinerary,
        location,
        location,
        starting_state,
        insulation_bps,
        has_tent,
    )
}

pub(super) fn projected_round_trip_thermal_safe(
    projection: RoundTripThermalProjection<'_>,
) -> Option<bool> {
    let RoundTripThermalProjection {
        starting_minute,
        outbound_itinerary,
        return_itinerary,
        action_minutes,
        route: PublicRoundTripRoute {
            origin,
            destination,
        },
        traveler:
            PublicThermalTraveler {
                starting_state,
                insulation_bps,
                has_tent,
            },
    } = projection;
    let outbound = projected_itinerary_thermal_state(
        starting_minute,
        outbound_itinerary,
        origin,
        destination,
        starting_state,
        insulation_bps,
        has_tent,
    )?;
    let action_start = starting_minute.saturating_add(outbound_itinerary.total_elapsed_minutes);
    let action = projected_stationary_outdoor_thermal_state(
        action_start,
        action_minutes,
        destination,
        outbound.state,
        insulation_bps,
    )?;
    let returned = projected_itinerary_thermal_state(
        action_start.saturating_add(action_minutes),
        return_itinerary,
        destination,
        origin,
        action.state,
        insulation_bps,
        has_tent,
    )?;
    Some(outbound.safe && action.safe && returned.safe)
}

pub(super) fn projected_recovery_round_trip_thermal_safe(
    projection: RoundTripThermalProjection<'_>,
    recovery_minutes: u64,
) -> Option<bool> {
    let RoundTripThermalProjection {
        starting_minute,
        outbound_itinerary,
        return_itinerary,
        action_minutes,
        route: PublicRoundTripRoute {
            origin,
            destination,
        },
        traveler:
            PublicThermalTraveler {
                starting_state,
                insulation_bps,
                has_tent,
            },
    } = projection;
    let outbound = projected_itinerary_thermal_state(
        starting_minute,
        outbound_itinerary,
        origin,
        destination,
        starting_state,
        insulation_bps,
        has_tent,
    )?;
    let recovery_start = starting_minute.saturating_add(outbound_itinerary.total_elapsed_minutes);
    let recovery = projected_stationary_field_thermal_state(
        recovery_start,
        recovery_minutes,
        destination,
        outbound.state,
        insulation_bps,
        has_tent,
    )?;
    let action_start = recovery_start.saturating_add(recovery_minutes);
    let action = projected_stationary_outdoor_thermal_state(
        action_start,
        action_minutes,
        destination,
        recovery.state,
        insulation_bps,
    )?;
    let returned = projected_itinerary_thermal_state(
        action_start.saturating_add(action_minutes),
        return_itinerary,
        destination,
        origin,
        action.state,
        insulation_bps,
        has_tent,
    )?;
    Some(outbound.safe && recovery.safe && action.safe && returned.safe)
}
