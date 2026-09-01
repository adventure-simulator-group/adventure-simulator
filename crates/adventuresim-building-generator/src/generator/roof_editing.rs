/// Recomputes an existing roof graph under its declared pivot policy.  A
/// child intersection that would need a new topological cut is rejected
/// explicitly instead of silently detaching the child.
pub fn set_roof_pitch(
    plan: &mut BuildingPlan,
    id: RoofAssemblyId,
    pitch_degrees: f32,
) -> Result<(), RoofEditError> {
    if !(15.0..=75.0).contains(&pitch_degrees) {
        return Err(RoofEditError::PitchOutsideProjectRange);
    }
    let assembly = plan
        .roof_assemblies
        .iter_mut()
        .find(|roof| roof.id == id)
        .ok_or(RoofEditError::MissingAssembly)?;
    if !assembly.children.is_empty() || assembly.parent.is_some() {
        return Err(RoofEditError::TopologyEvent);
    }
    let old_pitch = assembly
        .faces
        .first()
        .map_or(pitch_degrees, |face| face.pitch_degrees);
    if (old_pitch - pitch_degrees).abs() < 0.0001 {
        return Ok(());
    }
    let old_tan = old_pitch.to_radians().tan();
    if old_tan.abs() <= 0.0001 {
        return Err(RoofEditError::TopologyEvent);
    }
    let factor = pitch_degrees.to_radians().tan() / old_tan;
    let min_y = assembly
        .faces
        .iter()
        .flat_map(|face| face.polygon.iter().map(|point| point.y))
        .fold(f32::INFINITY, f32::min);
    let max_y = assembly
        .faces
        .iter()
        .flat_map(|face| face.polygon.iter().map(|point| point.y))
        .fold(f32::NEG_INFINITY, f32::max);
    let scale_y = |y: f32| match assembly.pivot_policy {
        RoofPivotPolicy::KeepEave | RoofPivotPolicy::KeepChildAttachment => {
            min_y + (y - min_y) * factor
        }
        RoofPivotPolicy::KeepRidge => max_y - (max_y - y) * factor,
    };
    for face in &mut assembly.faces {
        for point in &mut face.polygon {
            point.y = scale_y(point.y);
        }
        face.plane = roof_plane(&face.polygon);
        face.pitch_degrees = pitch_degrees;
        let bounds = roof_polygon_bounds(&face.polygon);
        if let Some(surface) = plan
            .resolved_geometry
            .surfaces
            .iter_mut()
            .find(|surface| surface.id == face.drainage_catchment)
        {
            surface.bounds = bounds;
        }
        if let Some(catchment) = plan
            .resolved_geometry
            .drainage_catchments
            .iter_mut()
            .find(|catchment| catchment.id == face.drainage_catchment)
        {
            let centre = face.polygon.iter().copied().sum::<Vec3>() / face.polygon.len() as f32;
            let low = face
                .polygon
                .iter()
                .min_by(|a, b| a.y.total_cmp(&b.y))
                .copied()
                .expect("roof face has vertices");
            catchment.centre = centre;
            catchment.inner_elevation_metres = face
                .polygon
                .iter()
                .map(|point| point.y)
                .fold(f32::NEG_INFINITY, f32::max);
            catchment.outer_elevation_metres = low.y;
            if let Some(route) = plan
                .resolved_geometry
                .drainage_routes
                .iter_mut()
                .find(|route| route.id == catchment.outlet_route)
            {
                route.inlet = centre;
                route.outlet = low;
            }
        }
    }
    for enclosure in &mut assembly.enclosure_faces {
        for point in &mut enclosure.polygon {
            if point.y > min_y + 0.01 {
                point.y = scale_y(point.y);
            }
        }
    }
    for edge in &mut assembly.edges {
        edge.start.y = scale_y(edge.start.y);
        edge.end.y = scale_y(edge.end.y);
    }
    for (edge_index, edge) in assembly.edges.iter().enumerate() {
        let delta = edge.end - edge.start;
        let plan_length = Vec2::new(delta.x, delta.z).length().max(0.05);
        let edge_pitch = delta.y.atan2(plan_length);
        let weather_id =
            ResolvedItemId((0x8_u64 << 60) | (assembly.id.0 << 16) | 0x5000 | edge_index as u64);
        if let Some(solid) = plan
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == weather_id)
        {
            let treated_plan_length = if edge.kind == RoofEdgeKind::Eave {
                (plan_length - 0.36_f32.min(plan_length * 0.5)).max(0.05)
            } else {
                plan_length
            };
            solid.centre = (edge.start + edge.end) * 0.5
                + if edge.kind == RoofEdgeKind::Eave {
                    Vec3::NEG_Y * 0.06
                } else {
                    Vec3::Y * 0.035
                };
            solid.size.x = if edge.kind == RoofEdgeKind::Eave {
                treated_plan_length
            } else {
                treated_plan_length / edge_pitch.cos().abs().max(0.01)
            };
            solid.yaw_radians = delta.z.atan2(delta.x);
            solid.longfall_radians = if edge.kind == RoofEdgeKind::Eave {
                0.012
            } else {
                edge_pitch
            };
        }
        if let Some(flashing_id) = edge.flashing
            && let Some(flashing) = plan
                .resolved_geometry
                .solids
                .iter_mut()
                .find(|solid| solid.id == flashing_id)
        {
            flashing.centre = (edge.start + edge.end) * 0.5 + Vec3::Y * (flashing.size.y * 0.5);
            flashing.size.x = delta.length().max(0.05);
            flashing.yaw_radians = delta.z.atan2(delta.x);
            flashing.longfall_radians = if edge.kind == RoofEdgeKind::Valley {
                edge_pitch
            } else {
                0.0
            };
        }
    }
    Ok(())
}
