//! Deterministic strategic route terrain derived from canonical edge geometry.
//!
//! Documented Viabundus edges use endpoint interpolation; inferred edges use
//! their terrain-routed schema-v25 polyline. This describes travel and encounter
//! selection only; tactical positions, combatants, HP, and tick state remain transient.

#[cfg(test)]
use std::collections::BTreeSet;
use std::{collections::BTreeMap, path::Path};

use adventuresim_world_schema::{
    CompiledWorld, CrossingWatercourse, DominantAspect, EdgeProgressPermille, FerryWaterway,
    LocatedRouteLandform, MAX_SOURCES_MARKDOWN_CHARS, RouteElevationProfile, RouteElevationSample,
    RouteLandformKind, RouteReliefMeters, RouteRoughnessMeters, RouteSignedGradePermille,
    RouteSlopePermille, RouteTerrain, RouteVerticalMeters, RouteWaterAdjacency,
    RouteWaterFeatureKind, TravelRoute, WaterDistanceMeters,
};
#[cfg(test)]
use adventuresim_world_schema::{
    CrossingTraversal, RouteEncounterTag, RouteRiskSeverity, RouteSeasonalHazard,
    RouteSeasonalRisk, RouteTerrainClass,
};

use super::elevation::Glo30Sampler;
use crate::{
    Error, Result,
    spatial::{ProjectedCoordinate, SpatialProjection},
};

#[derive(Clone, Copy)]
struct Local {
    elevation: adventuresim_world_schema::ElevationMeters,
    slope: u16,
    aspect: Option<f64>,
    tri: u16,
    landform: Option<RouteLandformKind>,
    fallback: usize,
}

pub(crate) fn enrich(
    mut world: CompiledWorld,
    elevation_directory: &Path,
) -> Result<CompiledWorld> {
    let projection = SpatialProjection::new()?;
    let nodes = world
        .nodes
        .iter()
        .map(|n| (n.id, (n.latitude, n.longitude)))
        .collect::<BTreeMap<_, _>>();
    let mut sampler = Glo30Sampler::new(elevation_directory);
    let mut dem_samples = 0usize;
    let mut dem_fallbacks = 0usize;
    for edge in &mut world.edges {
        let from = nodes
            .get(&edge.from_node_id)
            .ok_or_else(|| Error::Validation(format!("route {} has missing from node", edge.id)))?;
        let to = nodes
            .get(&edge.to_node_id)
            .ok_or_else(|| Error::Validation(format!("route {} has missing to node", edge.id)))?;
        let a = projection.project(from.0, from.1)?;
        let b = projection.project(to.0, to.1)?;
        let cell = world.metadata.spatial_grid.cell_size_meters().get();
        let segments = edge.length_m.div_ceil(cell).clamp(1, 1_000);
        let path = if edge.geometry.is_empty() {
            (0..=segments)
                .map(|index| interpolate(a, b, index, segments))
                .collect::<Result<Vec<_>>>()?
        } else {
            let vertices = edge
                .geometry
                .iter()
                .map(|point| projection.project(point.latitude(), point.longitude()))
                .collect::<Result<Vec<_>>>()?;
            densify_polyline(&vertices, cell)?
        };
        let mut local = Vec::with_capacity(path.len());
        for (index, point) in path.iter().copied().enumerate() {
            let previous = path[index.saturating_sub(1)];
            let next = path[(index + 1).min(path.len() - 1)];
            let route_direction = (
                next.easting_millimeters() - previous.easting_millimeters(),
                next.northing_millimeters() - previous.northing_millimeters(),
            );
            let sample = local_sample(&projection, &mut sampler, point, cell, route_direction)?;
            dem_samples += 9;
            dem_fallbacks += sample.fallback;
            local.push(sample);
        }
        let terrain = derive(
            &edge.route,
            edge.length_m,
            &local,
            &edge.terrain.water_adjacencies,
        )
        .map_err(Error::Validation)?;
        edge.terrain = terrain;
        let fallback = local.iter().any(|v| v.fallback > 0);
        let geometry = if edge.geometry.is_empty() {
            "straight endpoint geometry"
        } else {
            "canonical terrain-routed inferred geometry"
        };
        let note = if fallback {
            format!(
                "- **Route terrain rules v6:** {geometry} was sampled from GLO-30 with a 3x3 canonical-grid neighborhood; missing/void pixels used the bounded deterministic sea-level fallback. Nearest EU-Hydro facts within 2 km drive static seasonal/encounter tags. Viabundus slope_multiplier remains only a source cost hint."
            )
        } else {
            format!(
                "- **Route terrain rules v6:** {geometry} was sampled from GLO-30 with a 3x3 canonical-grid neighborhood. Nearest EU-Hydro facts within 2 km drive static seasonal/encounter tags. Viabundus slope_multiplier remains only a source cost hint."
            )
        };
        append_required_note(&mut edge.sources, &note, edge.id)?;
    }
    world.report.route_terrain_edges = world.edges.len();
    world.report.route_terrain_dem_samples = dem_samples;
    world.report.route_terrain_dem_fallbacks = dem_fallbacks;
    world.report.route_terrain_water_adjacencies = world
        .edges
        .iter()
        .map(|e| e.terrain.water_adjacencies.len())
        .sum();
    world.report.route_terrain_landforms =
        world.edges.iter().map(|e| e.terrain.landforms.len()).sum();
    world.report.route_terrain_seasonal_risks = world
        .edges
        .iter()
        .map(|e| e.terrain.seasonal_risks.len())
        .sum();
    world.report.route_terrain_encounter_tags = world
        .edges
        .iter()
        .map(|e| e.terrain.encounter_tags.len())
        .sum();
    Ok(world)
}

fn densify_polyline(
    vertices: &[ProjectedCoordinate],
    interval_m: u32,
) -> Result<Vec<ProjectedCoordinate>> {
    if vertices.len() < 2 || interval_m == 0 {
        return Err(Error::Validation(
            "route geometry cannot be densified".into(),
        ));
    }
    let mut output = vec![vertices[0]];
    for pair in vertices.windows(2) {
        let dx = pair[1].easting_millimeters() - pair[0].easting_millimeters();
        let dy = pair[1].northing_millimeters() - pair[0].northing_millimeters();
        let distance_mm = ((dx as f64).hypot(dy as f64)).ceil() as u64;
        let count = distance_mm.div_ceil(u64::from(interval_m) * 1_000).max(1);
        let count = u32::try_from(count)
            .map_err(|_| Error::Validation("route densification overflowed".into()))?;
        for index in 1..=count {
            output.push(interpolate(pair[0], pair[1], index, count)?);
        }
    }
    Ok(output)
}

fn append_required_note(sources: &mut String, note: &str, edge_id: u64) -> Result<()> {
    if sources.chars().count() + 1 + note.chars().count() > MAX_SOURCES_MARKDOWN_CHARS {
        return Err(Error::Validation(format!(
            "route {edge_id} has no room for required route-terrain provenance"
        )));
    }
    sources.push('\n');
    sources.push_str(note);
    Ok(())
}

fn interpolate(
    a: ProjectedCoordinate,
    b: ProjectedCoordinate,
    index: u32,
    count: u32,
) -> Result<ProjectedCoordinate> {
    let lerp = |x: i64, y: i64| -> Result<i64> {
        if index == 0 {
            return Ok(x);
        }
        if index == count {
            return Ok(y);
        }
        let numerator =
            i128::from(x) * i128::from(count - index) + i128::from(y) * i128::from(index);
        let denominator = i128::from(count);
        let magnitude = (numerator.unsigned_abs() + denominator as u128 / 2) / denominator as u128;
        let value = if numerator < 0 {
            -(magnitude as i128)
        } else {
            magnitude as i128
        };
        i64::try_from(value).map_err(|_| Error::Validation("route interpolation overflowed".into()))
    };
    ProjectedCoordinate::from_meters(
        lerp(a.easting_millimeters(), b.easting_millimeters())? as f64 / 1000.0,
        lerp(a.northing_millimeters(), b.northing_millimeters())? as f64 / 1000.0,
    )
}

fn local_sample(
    projection: &SpatialProjection,
    sampler: &mut Glo30Sampler<'_>,
    center: ProjectedCoordinate,
    cell: u32,
    route_direction: (i64, i64),
) -> Result<Local> {
    let mut z = [[0i32; 3]; 3];
    let mut fallback = 0;
    for (row, dy) in [-1i32, 0, 1].into_iter().enumerate() {
        for (col, dx) in [-1i32, 0, 1].into_iter().enumerate() {
            let point = ProjectedCoordinate::from_meters(
                center.easting_meters() + f64::from(dx) * f64::from(cell),
                center.northing_meters() + f64::from(dy) * f64::from(cell),
            )?;
            let (lat, lon) = projection.unproject(point)?;
            let (value, used) = sampler.sample(lat, lon)?;
            z[row][col] = i32::from(value.get());
            fallback += usize::from(used);
        }
    }
    let c = z[1][1];
    let gx = (z[0][2] + 2 * z[1][2] + z[2][2]) - (z[0][0] + 2 * z[1][0] + z[2][0]);
    let gy = (z[2][0] + 2 * z[2][1] + z[2][2]) - (z[0][0] + 2 * z[0][1] + z[0][2]);
    let scale = 8.0 * f64::from(cell);
    let east = f64::from(gx) / scale;
    let north = f64::from(gy) / scale;
    let slope = ((east.hypot(north) * 1000.0).round() as i64).clamp(0, 10_000) as u16;
    // Horn yields the uphill gradient. Aspect is the standard downslope bearing.
    let aspect = (slope >= 10).then(|| (-east).atan2(-north));
    let values = z.into_iter().flatten().collect::<Vec<_>>();
    let tri = values
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 4)
        .map(|(_, v)| (v - c).unsigned_abs())
        .sum::<u32>()
        / 8;
    let mean8 = values
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 4)
        .map(|(_, v)| *v)
        .sum::<i32>()
        / 8;
    let landform = classify_landform(z, mean8, route_direction);
    Ok(Local {
        elevation: adventuresim_world_schema::ElevationMeters::new(c.clamp(-500, 9000) as i16)
            .unwrap(),
        slope,
        aspect,
        tri: tri.min(9500) as u16,
        landform,
        fallback,
    })
}

fn classify_landform(
    z: [[i32; 3]; 3],
    mean8: i32,
    route_direction: (i64, i64),
) -> Option<RouteLandformKind> {
    let c = z[1][1];
    let scale = route_direction
        .0
        .unsigned_abs()
        .max(route_direction.1.unsigned_abs());
    let discrete = |value: i64| {
        if value.unsigned_abs().saturating_mul(2) >= scale {
            value.signum() as isize
        } else {
            0
        }
    };
    let dx = discrete(route_direction.0);
    let dy = discrete(route_direction.1);
    let forward = z[(1 + dy) as usize][(1 + dx) as usize];
    let backward = z[(1 - dy) as usize][(1 - dx) as usize];
    let left = z[(1 + dx) as usize][(1 - dy) as usize];
    let right = z[(1 - dx) as usize][(1 + dy) as usize];
    if forward <= c && backward <= c && left >= c + 20 && right >= c + 20 {
        Some(RouteLandformKind::LikelyPass)
    } else if c - mean8 >= 20 {
        Some(RouteLandformKind::Ridge)
    } else if mean8 - c >= 20 {
        Some(RouteLandformKind::Valley)
    } else {
        None
    }
}

fn derive(
    route: &TravelRoute,
    length_m: u32,
    local: &[Local],
    retained_water: &[RouteWaterAdjacency],
) -> std::result::Result<RouteTerrain, String> {
    if length_m == 0 {
        return Err("route terrain requires a positive edge length".into());
    }
    let last = local.len() - 1;
    let profile = local
        .iter()
        .enumerate()
        .map(|(i, v)| RouteElevationSample {
            progress: EdgeProgressPermille::new(((i * 1000 + last / 2) / last) as u16).unwrap(),
            elevation: v.elevation,
        })
        .collect();
    let profile = RouteElevationProfile::new(profile)?;
    let mut ascent = 0u32;
    let mut descent = 0u32;
    let mut up = 0i16;
    let mut down = 0i16;
    for pair in profile.samples().windows(2) {
        let dz = i32::from(pair[1].elevation.get()) - i32::from(pair[0].elevation.get());
        if dz >= 0 {
            ascent = ascent.saturating_add(dz as u32)
        } else {
            descent = descent.saturating_add((-dz) as u32)
        };
        let grade = adventuresim_world_schema::route_grade_permille(
            dz,
            length_m,
            u32::from(pair[1].progress.get() - pair[0].progress.get()),
        )?;
        up = up.max(grade);
        down = down.min(grade);
    }
    let mean_slope =
        (local.iter().map(|v| u32::from(v.slope)).sum::<u32>() / local.len() as u32) as u16;
    let max_slope = local.iter().map(|v| v.slope).max().unwrap_or(0);
    let relief = (i32::from(local.iter().map(|v| v.elevation.get()).max().unwrap())
        - i32::from(local.iter().map(|v| v.elevation.get()).min().unwrap()))
    .clamp(0, 9500) as u16;
    let class = RouteTerrain::class_for(max_slope, relief);
    let (sin, cos, n) = local
        .iter()
        .filter_map(|v| v.aspect)
        .fold((0.0, 0.0, 0u32), |(s, c, n), a| {
            (s + a.sin(), c + a.cos(), n + 1)
        });
    let dominant_aspect = if mean_slope < 10 || n == 0 {
        DominantAspect::Flat
    } else {
        aspect_bucket(sin.atan2(cos))
    };
    let mut landforms = Vec::new();
    let mut previous = None;
    for (i, value) in local.iter().enumerate() {
        if value.landform == previous {
            continue;
        }
        previous = value.landform;
        if let Some(kind) = value.landform {
            landforms.push(LocatedRouteLandform {
                progress: EdgeProgressPermille::new(((i * 1000 + last / 2) / last) as u16).unwrap(),
                kind,
            });
        }
    }
    let water = water_context(route, retained_water);
    let risks = adventuresim_world_schema::expected_route_seasonal_risks(
        route,
        class,
        &water,
        &landforms,
        local.iter().map(|v| v.elevation.get()).max().unwrap(),
    );
    let roughness = (local.iter().map(|v| u64::from(v.tri)).sum::<u64>() / local.len() as u64)
        .min(u64::from(RouteRoughnessMeters::MAX)) as u16;
    let tags = adventuresim_world_schema::expected_route_encounter_tags(
        route, class, max_slope, roughness, &landforms, &water, &risks,
    );
    let value = RouteTerrain {
        elevation_profile: profile,
        ascent: RouteVerticalMeters::new(ascent.min(100000))?,
        descent: RouteVerticalMeters::new(descent.min(100000))?,
        max_uphill_grade: RouteSignedGradePermille::new(up.max(0))?,
        max_downhill_grade: RouteSignedGradePermille::new(down.min(0))?,
        mean_slope: RouteSlopePermille::new(mean_slope)?,
        max_slope: RouteSlopePermille::new(max_slope)?,
        dominant_aspect,
        roughness: RouteRoughnessMeters::new(roughness)?,
        relief: RouteReliefMeters::new(relief)?,
        landforms,
        class,
        water_adjacencies: water,
        seasonal_risks: risks,
        encounter_tags: tags,
    };
    value.validate_context(route, length_m)?;
    Ok(value)
}

fn aspect_bucket(a: f64) -> DominantAspect {
    let oct = ((a.to_degrees() + 360.0 + 22.5) % 360.0 / 45.0) as u8;
    match oct {
        0 => DominantAspect::North,
        1 => DominantAspect::NorthEast,
        2 => DominantAspect::East,
        3 => DominantAspect::SouthEast,
        4 => DominantAspect::South,
        5 => DominantAspect::SouthWest,
        6 => DominantAspect::West,
        _ => DominantAspect::NorthWest,
    }
}
fn water_context(
    route: &TravelRoute,
    retained: &[RouteWaterAdjacency],
) -> Vec<RouteWaterAdjacency> {
    let mut by_kind = BTreeMap::new();
    for value in retained {
        by_kind.insert(value.feature, value.distance);
    }
    let mut zero = |feature| {
        by_kind.insert(feature, WaterDistanceMeters::new(0).unwrap());
    };
    match route {
        TravelRoute::Land(l) => {
            for c in &l.water_crossings {
                let feature = match c.watercourse {
                    CrossingWatercourse::River(_) => RouteWaterFeatureKind::River,
                    CrossingWatercourse::Canal(_) => RouteWaterFeatureKind::Canal,
                    CrossingWatercourse::Ditch => RouteWaterFeatureKind::Ditch,
                };
                zero(feature);
            }
        }
        TravelRoute::Ferry(f) => {
            let feature = match f.waterway {
                FerryWaterway::River(_) => RouteWaterFeatureKind::River,
                FerryWaterway::InlandWater => RouteWaterFeatureKind::Inland,
                FerryWaterway::TidalWater => RouteWaterFeatureKind::Tidal,
                FerryWaterway::CoastalWater => RouteWaterFeatureKind::Coastal,
            };
            zero(feature);
        }
    }
    by_kind
        .into_iter()
        .map(|(feature, distance)| RouteWaterAdjacency { feature, distance })
        .collect()
}
#[cfg(test)]
fn seasonal(
    route: &TravelRoute,
    class: RouteTerrainClass,
    water: &[RouteWaterAdjacency],
    landforms: &[LocatedRouteLandform],
    max_elev: i16,
) -> Vec<RouteSeasonalRisk> {
    let ford = matches!(route,TravelRoute::Land(l) if l.water_crossings.iter().any(|c|c.traversal==CrossingTraversal::Ford));
    let ferry = matches!(route, TravelRoute::Ferry(_));
    let valley = landforms
        .iter()
        .any(|value| value.kind == RouteLandformKind::Valley);
    let flowing = water.iter().any(|w| {
        matches!(
            w.feature,
            RouteWaterFeatureKind::River
                | RouteWaterFeatureKind::Canal
                | RouteWaterFeatureKind::Ditch
                | RouteWaterFeatureKind::Inland
                | RouteWaterFeatureKind::Tidal
        ) && w.distance.get() <= 500
    });
    let fresh250 = water.iter().any(|w| {
        matches!(
            w.feature,
            RouteWaterFeatureKind::River
                | RouteWaterFeatureKind::Canal
                | RouteWaterFeatureKind::Ditch
                | RouteWaterFeatureKind::Inland
        ) && w.distance.get() <= 250
    });
    let inland250 = water.iter().any(|w| {
        matches!(
            w.feature,
            RouteWaterFeatureKind::Inland | RouteWaterFeatureKind::Tidal
        ) && w.distance.get() <= 250
    });
    let mut s = BTreeSet::new();
    if ford {
        s.insert(RouteSeasonalRisk {
            hazard: RouteSeasonalHazard::SpringFlood,
            severity: RouteRiskSeverity::High,
        });
    } else if valley && flowing {
        s.insert(RouteSeasonalRisk {
            hazard: RouteSeasonalHazard::SpringFlood,
            severity: RouteRiskSeverity::Medium,
        });
    }
    if ford {
        s.insert(RouteSeasonalRisk {
            hazard: RouteSeasonalHazard::AutumnMud,
            severity: RouteRiskSeverity::Medium,
        });
    } else if fresh250 && matches!(class, RouteTerrainClass::Flat | RouteTerrainClass::Rolling) {
        s.insert(RouteSeasonalRisk {
            hazard: RouteSeasonalHazard::AutumnMud,
            severity: RouteRiskSeverity::Low,
        });
    }
    if ferry {
        s.insert(RouteSeasonalRisk {
            hazard: RouteSeasonalHazard::WinterIce,
            severity: RouteRiskSeverity::High,
        });
    } else if ford {
        s.insert(RouteSeasonalRisk {
            hazard: RouteSeasonalHazard::WinterIce,
            severity: RouteRiskSeverity::Medium,
        });
    } else if inland250 {
        s.insert(RouteSeasonalRisk {
            hazard: RouteSeasonalHazard::WinterIce,
            severity: RouteRiskSeverity::Low,
        });
    }
    if class == RouteTerrainClass::Mountainous || max_elev >= 1000 {
        s.insert(RouteSeasonalRisk {
            hazard: RouteSeasonalHazard::WinterSnow,
            severity: RouteRiskSeverity::Medium,
        });
    }
    s.into_iter().collect()
}
#[cfg(test)]
fn tags(
    route: &TravelRoute,
    class: RouteTerrainClass,
    max_slope: u16,
    tri: u16,
    land: &[LocatedRouteLandform],
    water: &[RouteWaterAdjacency],
    risks: &[RouteSeasonalRisk],
) -> Vec<RouteEncounterTag> {
    let mut s = BTreeSet::new();
    s.insert(match class {
        RouteTerrainClass::Flat => RouteEncounterTag::Flat,
        RouteTerrainClass::Rolling => RouteEncounterTag::Rolling,
        RouteTerrainClass::Hilly => RouteEncounterTag::Hilly,
        RouteTerrainClass::Mountainous => RouteEncounterTag::Mountainous,
    });
    if max_slope >= 150 {
        s.insert(RouteEncounterTag::Steep);
    }
    if tri >= 20 {
        s.insert(RouteEncounterTag::Rough);
    }
    for l in land {
        s.insert(match l.kind {
            RouteLandformKind::Ridge => RouteEncounterTag::Ridge,
            RouteLandformKind::Valley => RouteEncounterTag::Valley,
            RouteLandformKind::LikelyPass => RouteEncounterTag::LikelyPass,
        });
    }
    match route {
        TravelRoute::Ferry(_) => {
            s.insert(RouteEncounterTag::Ferry);
        }
        TravelRoute::Land(l) => {
            for c in &l.water_crossings {
                s.insert(if c.traversal == CrossingTraversal::Bridge {
                    RouteEncounterTag::Bridge
                } else {
                    RouteEncounterTag::Ford
                });
            }
        }
    }
    for w in water {
        s.insert(match w.feature {
            RouteWaterFeatureKind::River | RouteWaterFeatureKind::Ditch => {
                RouteEncounterTag::Riverbank
            }
            RouteWaterFeatureKind::Canal => RouteEncounterTag::CanalBank,
            RouteWaterFeatureKind::Inland => RouteEncounterTag::Lakeshore,
            RouteWaterFeatureKind::Tidal => RouteEncounterTag::TidalShore,
            RouteWaterFeatureKind::Coastal => RouteEncounterTag::Coast,
        });
    }
    for r in risks {
        s.insert(match r.hazard {
            RouteSeasonalHazard::SpringFlood => RouteEncounterTag::SpringFlood,
            RouteSeasonalHazard::AutumnMud => RouteEncounterTag::AutumnMud,
            RouteSeasonalHazard::WinterIce => RouteEncounterTag::WinterIce,
            RouteSeasonalHazard::WinterSnow => RouteEncounterTag::WinterSnow,
        });
    }
    s.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn local(elevation: i16) -> Local {
        Local {
            elevation: adventuresim_world_schema::ElevationMeters::new(elevation).unwrap(),
            slope: 0,
            aspect: None,
            tri: 0,
            landform: None,
            fallback: 0,
        }
    }

    #[test]
    fn aspect_buckets_cardinals() {
        assert_eq!(aspect_bucket(0.0), DominantAspect::North);
        assert_eq!(
            aspect_bucket(std::f64::consts::FRAC_PI_2),
            DominantAspect::East
        );
    }

    #[test]
    fn profile_endpoints_cap_and_directional_grades_are_deterministic() {
        let route = TravelRoute::Land(adventuresim_world_schema::LandRoute {
            bridge: None,
            water_crossings: vec![],
        });
        let uphill = derive(&route, 2_000, &[local(0), local(100), local(300)], &[]).unwrap();
        assert_eq!(
            uphill
                .elevation_profile
                .samples()
                .first()
                .unwrap()
                .progress
                .get(),
            0
        );
        assert_eq!(
            uphill
                .elevation_profile
                .samples()
                .last()
                .unwrap()
                .progress
                .get(),
            1_000
        );
        assert_eq!(uphill.ascent.get(), 300);
        assert_eq!(uphill.descent.get(), 0);
        assert_eq!(uphill.max_uphill_grade.get(), 200);

        let downhill = derive(&route, 2_000, &[local(300), local(100), local(0)], &[]).unwrap();
        assert_eq!(downhill.ascent.get(), 0);
        assert_eq!(downhill.descent.get(), 300);
        assert_eq!(downhill.max_downhill_grade.get(), -200);
    }

    #[test]
    fn zero_length_route_is_rejected_without_grade_arithmetic() {
        let route = TravelRoute::Land(adventuresim_world_schema::LandRoute {
            bridge: None,
            water_crossings: vec![],
        });
        assert!(derive(&route, 0, &[local(0), local(1)], &[]).is_err());
    }

    #[test]
    fn ford_seasonal_and_encounter_table_is_exact() {
        let route = TravelRoute::Land(adventuresim_world_schema::LandRoute {
            bridge: None,
            water_crossings: vec![adventuresim_world_schema::LandWaterCrossing {
                position: EdgeProgressPermille::new(500).unwrap(),
                watercourse: CrossingWatercourse::Ditch,
                traversal: CrossingTraversal::Ford,
            }],
        });
        let terrain = derive(&route, 1_000, &[local(0), local(0)], &[]).unwrap();
        assert_eq!(
            terrain.seasonal_risks,
            vec![
                RouteSeasonalRisk {
                    hazard: RouteSeasonalHazard::SpringFlood,
                    severity: RouteRiskSeverity::High
                },
                RouteSeasonalRisk {
                    hazard: RouteSeasonalHazard::AutumnMud,
                    severity: RouteRiskSeverity::Medium
                },
                RouteSeasonalRisk {
                    hazard: RouteSeasonalHazard::WinterIce,
                    severity: RouteRiskSeverity::Medium
                },
            ]
        );
        assert!(terrain.encounter_tags.contains(&RouteEncounterTag::Ford));
        assert!(
            terrain
                .encounter_tags
                .contains(&RouteEncounterTag::Riverbank)
        );
        assert_eq!(
            terrain.seasonal_risks,
            seasonal(
                &route,
                terrain.class,
                &terrain.water_adjacencies,
                &terrain.landforms,
                0,
            )
        );
        assert_eq!(
            terrain.encounter_tags,
            tags(
                &route,
                terrain.class,
                terrain.max_slope.get(),
                terrain.roughness.get(),
                &terrain.landforms,
                &terrain.water_adjacencies,
                &terrain.seasonal_risks,
            )
        );
    }

    #[test]
    fn likely_pass_requires_route_aligned_low_and_orthogonal_high_neighbors() {
        let saddle = [[0, 30, 0], [0, 10, 0], [0, 30, 0]];
        assert_eq!(
            classify_landform(saddle, 7, (1, 0)),
            Some(RouteLandformKind::LikelyPass)
        );
        assert_ne!(
            classify_landform(saddle, 7, (0, 1)),
            Some(RouteLandformKind::LikelyPass)
        );
    }

    #[test]
    fn interpolation_is_endpoint_exact_and_reversal_symmetric_on_both_axes() {
        let a = ProjectedCoordinate::from_meters(-0.002, 0.001).unwrap();
        let b = ProjectedCoordinate::from_meters(0.003, -0.004).unwrap();
        assert_eq!(interpolate(a, b, 0, 4).unwrap(), a);
        assert_eq!(interpolate(a, b, 4, 4).unwrap(), b);
        let forward = (0..=4)
            .map(|i| interpolate(a, b, i, 4).unwrap())
            .collect::<Vec<_>>();
        let mut reverse = (0..=4)
            .map(|i| interpolate(b, a, i, 4).unwrap())
            .collect::<Vec<_>>();
        reverse.reverse();
        assert_eq!(forward, reverse);
        assert_eq!(forward[2].easting_millimeters(), 1);
        assert_eq!(forward[2].northing_millimeters(), -2);
    }

    #[test]
    fn inferred_polyline_densification_preserves_collinear_ridge_vertex_and_midpoints() {
        let vertices = [
            ProjectedCoordinate::from_meters(0.0, 0.0).unwrap(),
            ProjectedCoordinate::from_meters(1_000.0, 0.0).unwrap(),
            ProjectedCoordinate::from_meters(2_000.0, 0.0).unwrap(),
        ];
        let samples = densify_polyline(&vertices, 500).unwrap();
        assert_eq!(samples.len(), 5);
        assert_eq!(samples[2], vertices[1]);
        assert_eq!(samples[1].easting_meters(), 500.0);
        assert_eq!(samples[3].easting_meters(), 1_500.0);
    }

    #[test]
    fn long_rough_profile_uses_wide_accumulators() {
        let route = TravelRoute::Land(adventuresim_world_schema::LandRoute {
            bridge: None,
            water_crossings: vec![],
        });
        let mut values = vec![local(0); 1_001];
        for value in &mut values {
            value.tri = 9_500;
        }
        let terrain = derive(&route, 1_000_000, &values, &[]).unwrap();
        assert_eq!(terrain.roughness.get(), 9_500);
        assert!(terrain.encounter_tags.contains(&RouteEncounterTag::Rough));
    }

    #[test]
    fn landform_runs_collapse_before_awkward_progress_conversion() {
        let route = TravelRoute::Land(adventuresim_world_schema::LandRoute {
            bridge: None,
            water_crossings: vec![],
        });
        let mut values = vec![local(0); 334];
        for value in &mut values[1..101] {
            value.landform = Some(RouteLandformKind::Ridge);
        }
        let terrain = derive(&route, 333_000, &values, &[]).unwrap();
        assert_eq!(terrain.landforms.len(), 1);
        assert_eq!(terrain.landforms[0].progress.get(), 3);
    }

    #[test]
    fn required_provenance_fails_closed_at_bound() {
        let mut sources = "x".repeat(MAX_SOURCES_MARKDOWN_CHARS - 2);
        assert!(append_required_note(&mut sources, "- note", 7).is_err());
        assert_eq!(sources.len(), MAX_SOURCES_MARKDOWN_CHARS - 2);
    }

    #[test]
    fn downslope_aspect_reverses_horn_uphill_gradient() {
        let north_rising = [[0, 0, 0], [10, 10, 10], [20, 20, 20]];
        let c = north_rising[1][1];
        let gx = (north_rising[0][2] + 2 * north_rising[1][2] + north_rising[2][2])
            - (north_rising[0][0] + 2 * north_rising[1][0] + north_rising[2][0]);
        let gy = (north_rising[2][0] + 2 * north_rising[2][1] + north_rising[2][2])
            - (north_rising[0][0] + 2 * north_rising[0][1] + north_rising[0][2]);
        let _ = c;
        let aspect = (-(gx as f64)).atan2(-(gy as f64));
        assert_eq!(aspect_bucket(aspect), DominantAspect::South);
    }
}
