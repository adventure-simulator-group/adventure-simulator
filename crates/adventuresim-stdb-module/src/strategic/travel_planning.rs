use adventuresim_world_schema::coordinates::{UnboundedCoordinateE7, Wgs84CoordinateE7};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StrategicPositionE7 {
    longitude_e7: i32,
    latitude_e7: i32,
}

/// Encodes the hybrid strategic-location wire representation. Geographic
/// positions are validated as WGS84; explicitly abstract positions preserve
/// their unbounded planar coordinate convention.
fn encode_position_e7(
    longitude: f64,
    latitude: f64,
    coordinates_are_geographic: bool,
) -> Option<StrategicPositionE7> {
    if coordinates_are_geographic {
        let coordinate = Wgs84CoordinateE7::from_longitude_latitude_degrees(longitude, latitude)?;
        Some(StrategicPositionE7 {
            longitude_e7: coordinate.longitude().get(),
            latitude_e7: coordinate.latitude().get(),
        })
    } else {
        let longitude = UnboundedCoordinateE7::from_coordinate_units(longitude)?;
        let latitude = UnboundedCoordinateE7::from_coordinate_units(latitude)?;
        Some(StrategicPositionE7 {
            longitude_e7: longitude.raw(),
            latitude_e7: latitude.raw(),
        })
    }
}

/// Decodes the hybrid strategic-location wire representation. Invalid WGS84
/// values fail closed instead of entering distance or route calculations.
fn decode_position_e7(
    longitude_e7: i32,
    latitude_e7: i32,
    coordinates_are_geographic: bool,
) -> Option<(f64, f64)> {
    if coordinates_are_geographic {
        Wgs84CoordinateE7::new(latitude_e7, longitude_e7)
            .map(|coordinate| coordinate.longitude_latitude_degrees())
    } else {
        Some((
            UnboundedCoordinateE7::from_raw(longitude_e7).coordinate_units(),
            UnboundedCoordinateE7::from_raw(latitude_e7).coordinate_units(),
        ))
    }
}

fn wgs84_route_coordinate(point: &JourneyRoutePoint) -> Option<Wgs84CoordinateE7> {
    Wgs84CoordinateE7::new(point.latitude_e7, point.longitude_e7)
}

fn travel_neighbors(ctx: &ReducerContext, node: u64) -> Vec<(u64, u32)> {
    let mut neighbors: Vec<_> = ctx
        .db
        .travel_edge()
        .from_node_id()
        .filter(&node)
        .map(|edge| (edge.to_node_id, edge.length_m))
        .collect();
    neighbors.extend(
        ctx.db
            .travel_edge()
            .to_node_id()
            .filter(&node)
            .map(|edge| (edge.from_node_id, edge.length_m)),
    );
    neighbors
}

/// Returns the next settlements reached from a source. Paths end at the first
/// settlement encountered, so journeys cannot skip intermediate settlements.
fn connected_settlement_distances(ctx: &ReducerContext, source_node_id: u64) -> HashMap<u64, u64> {
    let settlement_nodes: HashSet<u64> = ctx
        .db
        .settlement()
        .iter()
        .filter_map(|settlement| settlement.source_node_id)
        .collect();
    let mut distances = HashMap::from([(source_node_id, 0_u64)]);
    let mut pending = BinaryHeap::from([std::cmp::Reverse((0_u64, source_node_id))]);
    let mut destinations = HashMap::new();

    while let Some(std::cmp::Reverse((distance, node))) = pending.pop() {
        if distances.get(&node).is_some_and(|known| *known != distance) {
            continue;
        }
        if node != source_node_id && settlement_nodes.contains(&node) {
            destinations.insert(node, distance);
            continue;
        }
        for (neighbor, length_m) in travel_neighbors(ctx, node) {
            let next_distance = distance.saturating_add(u64::from(length_m));
            if distances
                .get(&neighbor)
                .is_none_or(|known| next_distance < *known)
            {
                distances.insert(neighbor, next_distance);
                pending.push(std::cmp::Reverse((next_distance, neighbor)));
            }
        }
    }
    destinations
}

fn journey_minutes(distance_m: u64) -> u64 {
    distance_m
        .saturating_mul(MINUTES_PER_HOUR)
        .div_ceil(
            adventuresim_core::strategic_time::OVERLAND_WALKING_SPEED_KM_PER_HOUR
                * METERS_PER_KILOMETER,
        )
        .max(1)
}

fn quest_journey_minutes(distance_m: u64) -> u64 {
    journey_minutes(distance_m).saturating_mul(QUEST_TRAVEL_SPEED_DIVISOR)
}

fn validate_journey_route(
    ctx: &ReducerContext,
    route: &JourneyRoutePlan,
    origin: (f64, f64),
    destination: (f64, f64),
) -> Result<(), String> {
    let authority = require_strategic_gateway(ctx)?;
    if authority.terrain_schema != 3
        || authority.terrain_package_digest.as_deref() != Some(route.package_digest.as_str())
    {
        return Err("Terrain route does not match the gateway terrain package".into());
    }
    validate_journey_route_payload(route, origin, destination)
}

fn validate_journey_route_payload(
    route: &JourneyRoutePlan,
    origin: (f64, f64),
    destination: (f64, f64),
) -> Result<(), String> {
    const MAX_POINTS: usize = 512;
    const MAX_SPANS: usize = 256;
    if !valid_route_digest(&route.package_digest) {
        return Err("Terrain route has an invalid package digest".into());
    }
    if route.weather_rules_version != adventuresim_core::weather::WEATHER_RULES_VERSION
        || !route
            .weather_interval_start
            .is_multiple_of(adventuresim_core::weather::WEATHER_INTERVAL_MINUTES)
        || route.intensity_bps > adventuresim_world_schema::BASIS_POINTS_PER_WHOLE
        || route.ground_moisture_bps > adventuresim_world_schema::BASIS_POINTS_PER_WHOLE
        || route.snow_cover_bps > adventuresim_world_schema::BASIS_POINTS_PER_WHOLE
        || (route.precipitation == JourneyPrecipitation::Clear && route.intensity_bps != 0)
    {
        return Err("Terrain route has an invalid weather departure snapshot".into());
    }
    if !(2..=MAX_POINTS).contains(&route.points.len())
        || route.spans.is_empty()
        || route.spans.len() > MAX_SPANS
        || route.distance_m == 0
        || route.distance_m > 2_000_000
        || route.minutes == 0
        || route.minutes > 2_000_000
    {
        return Err("Terrain route exceeds its collection or aggregate bounds".into());
    }
    let Some(coordinates) = route
        .points
        .iter()
        .map(wgs84_route_coordinate)
        .collect::<Option<Vec<_>>>()
    else {
        return Err("Terrain route contains an invalid coordinate".into());
    };
    let first = coordinates
        .first()
        .expect("bounded nonempty route")
        .longitude_latitude_degrees();
    let last = coordinates
        .last()
        .expect("bounded nonempty route")
        .longitude_latitude_degrees();
    if straight_line_distance_m(first.0, first.1, origin.0, origin.1, true) > 500
        || straight_line_distance_m(last.0, last.1, destination.0, destination.1, true) > 500
    {
        return Err("Terrain route endpoints do not match the current journey".into());
    }
    let mut physical = 0_u64;
    for pair in coordinates.windows(2) {
        let from = pair[0].longitude_latitude_degrees();
        let to = pair[1].longitude_latitude_degrees();
        let segment = straight_line_distance_m(from.0, from.1, to.0, to.1, true);
        if segment == 0 || segment > 100_000 {
            return Err("Terrain route points are not a bounded continuous path".into());
        }
        physical = physical
            .checked_add(segment)
            .ok_or("Terrain route distance overflow")?;
    }
    let tolerance = route.distance_m / 20 + 250;
    if physical.abs_diff(route.distance_m) > tolerance {
        return Err("Terrain route distance does not match its geometry".into());
    }
    let minimum_minutes = route
        .distance_m
        .saturating_mul(MINUTES_PER_HOUR)
        .div_ceil(7_500)
        .max(1);
    if route.minutes < minimum_minutes {
        return Err("Terrain route duration is faster than the maximum travel speed".into());
    }
    let mut cursor = 0_u64;
    for span in &route.spans {
        let weight_sum = u32::from(span.terrain.plains)
            + u32::from(span.terrain.forest)
            + u32::from(span.terrain.hills)
            + u32::from(span.terrain.wetlands)
            + u32::from(span.terrain.urban);
        if weight_sum != 1_000
            || span.terrain.urban != 0
            || span.training_multiplier_permille > 1_000
            || span.check_millirank > 5_000
        {
            return Err("Terrain route span has invalid bounded skill metadata".into());
        }
        if span.start_minute != cursor || span.duration_minutes == 0 {
            return Err("Terrain route spans are discontinuous".into());
        }
        cursor = cursor
            .checked_add(span.duration_minutes)
            .ok_or("Terrain route minutes overflow")?;
    }
    if cursor != route.minutes {
        return Err("Terrain route spans do not match aggregate minutes".into());
    }
    Ok(())
}

fn validate_route_departure_weather_interval(
    route: &JourneyRoutePlan,
    departure_minute: u64,
) -> Result<(), String> {
    let expected = departure_minute / adventuresim_core::weather::WEATHER_INTERVAL_MINUTES
        * adventuresim_core::weather::WEATHER_INTERVAL_MINUTES;
    if route.weather_interval_start != expected {
        return Err("Terrain route weather snapshot is stale after clock synchronization".into());
    }
    Ok(())
}

fn validate_camp_redirect_weather_interval(
    route: &JourneyRoutePlan,
    redirect_departure_minute: u64,
) -> Result<(), String> {
    validate_route_departure_weather_interval(route, redirect_departure_minute)
}

fn validate_return_journey_route(
    ctx: &ReducerContext,
    route: &JourneyRoutePlan,
    origin: (f64, f64),
    destination: (f64, f64),
) -> Result<(), String> {
    let leg = route
        .return_route
        .as_ref()
        .ok_or("Quest travel requires an independently planned return route")?;
    validate_journey_route(
        ctx,
        &JourneyRoutePlan {
            package_digest: route.package_digest.clone(),
            weather_rules_version: route.weather_rules_version,
            weather_interval_start: route.weather_interval_start,
            precipitation: route.precipitation,
            intensity_bps: route.intensity_bps,
            ground_moisture_bps: route.ground_moisture_bps,
            snow_cover_bps: route.snow_cover_bps,
            distance_m: leg.distance_m,
            minutes: leg.minutes,
            points: leg.points.clone(),
            spans: leg.spans.clone(),
            return_route: None,
        },
        origin,
        destination,
    )
}

fn straight_line_distance_m(
    from_x: f64,
    from_y: f64,
    to_x: f64,
    to_y: f64,
    geographic: bool,
) -> u64 {
    if geographic {
        let earth_radius_m = 6_371_000.0_f64;
        let lat1 = from_y.to_radians();
        let lat2 = to_y.to_radians();
        let delta_lat = (to_y - from_y).to_radians();
        let delta_lon = (to_x - from_x).to_radians();
        let a = (delta_lat / 2.0).sin().powi(2)
            + lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2);
        if !a.is_finite() {
            return u64::MAX;
        }
        let a = a.clamp(0.0, 1.0);
        let distance_m = earth_radius_m * 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
        if distance_m.is_finite() {
            distance_m.round() as u64
        } else {
            u64::MAX
        }
    } else {
        (((from_x - to_x).powi(2) + (from_y - to_y).powi(2)).sqrt() * METERS_PER_KILOMETER as f64)
            .round() as u64
    }
}

/// Canonical travel distance between signed E7 coordinates, in meters.
/// Geographic mode uses great-circle distance. Abstract mode uses the same
/// Euclidean coordinate-units-as-kilometers convention as strategic travel.
/// Invalid geographic latitude/longitude values fail closed.
pub(crate) fn coordinate_distance_e7_m(
    from_longitude_e7: i32,
    from_latitude_e7: i32,
    to_longitude_e7: i32,
    to_latitude_e7: i32,
    coordinates_are_geographic: bool,
) -> Option<u64> {
    let from = decode_position_e7(
        from_longitude_e7,
        from_latitude_e7,
        coordinates_are_geographic,
    )?;
    let to = decode_position_e7(to_longitude_e7, to_latitude_e7, coordinates_are_geographic)?;
    Some(straight_line_distance_m(
        from.0,
        from.1,
        to.0,
        to.1,
        coordinates_are_geographic,
    ))
}
