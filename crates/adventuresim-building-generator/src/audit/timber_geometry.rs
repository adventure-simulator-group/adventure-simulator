fn timber_audit_polygon(points: impl IntoIterator<Item = Vec2>) -> Polygon<f32> {
    let mut coordinates = points
        .into_iter()
        .map(|point| Coord {
            x: point.x,
            y: point.y,
        })
        .collect::<Vec<_>>();
    if coordinates.first() != coordinates.last() {
        coordinates.push(coordinates[0]);
    }
    Polygon::new(LineString::new(coordinates), Vec::new())
}

fn timber_member_audit_polygon(
    member: &crate::TimberFrameMember,
    wall: &crate::WallAssembly,
) -> Polygon<f32> {
    let project = |point: Vec3| {
        Vec2::new(
            (Vec2::new(point.x, point.z) - wall.frame.origin).dot(wall.frame.tangent),
            point.y - wall.base_elevation_metres,
        )
    };
    let start = project(member.start);
    let end = project(member.end);
    let axis = (end - start).normalize_or_zero();
    let normal = Vec2::new(-axis.y, axis.x);
    let half = (member.section_metres.max_element() * 0.5
        - crate::TIMBER_INFILL_EDGE_UNDERLAP_METRES)
        .max(0.0);
    timber_audit_polygon([
        start - normal * half,
        end - normal * half,
        end + normal * half,
        start + normal * half,
    ])
}

fn timber_panel_audit_polygon(
    solid: &ResolvedSolid,
    wall: &crate::WallAssembly,
) -> Option<Polygon<f32>> {
    let crate::ResolvedSolidShape::TimberPanelPrism {
        vertices,
        outward,
        depth_metres,
    } = solid.shape
    else {
        return None;
    };
    let expected_depth =
        (wall.thickness_metres - crate::TIMBER_INFILL_FINISH_SETBACK_METRES).max(0.04);
    if outward.dot(wall.frame.outward) < 0.999
        || depth_metres <= 0.02
        || (depth_metres - expected_depth).abs() > 0.002
    {
        return None;
    }
    let depth_offset = Vec3::new(outward.x, 0.0, outward.y) * depth_metres * 0.5;
    let min = vertices
        .iter()
        .flat_map(|vertex| [*vertex - depth_offset, *vertex + depth_offset])
        .fold(Vec3::splat(f32::INFINITY), Vec3::min);
    let max = vertices
        .iter()
        .flat_map(|vertex| [*vertex - depth_offset, *vertex + depth_offset])
        .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
    if solid.centre.distance((min + max) * 0.5) > 0.002 || solid.size.distance(max - min) > 0.002 {
        return None;
    }
    Some(timber_audit_polygon(vertices.map(|vertex| {
        Vec2::new(
            (Vec2::new(vertex.x, vertex.z) - wall.frame.origin).dot(wall.frame.tangent),
            vertex.y - wall.base_elevation_metres,
        )
    })))
}

fn timber_infill_residual_valid(
    plan: &BuildingPlan,
    frame: &crate::TimberFrameAssembly,
    wall: &crate::WallAssembly,
    bay: &crate::TimberFrameBay,
    solids: &std::collections::HashMap<ResolvedItemId, &ResolvedSolid>,
) -> bool {
    let declared = bay
        .infill_solids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let authoritative = wall
        .host_solids
        .iter()
        .copied()
        .filter(|id| {
            solids
                .get(id)
                .is_some_and(|solid| solid.role == SolidRole::WallHost)
        })
        .collect::<std::collections::HashSet<_>>();
    let panels = bay
        .infill_solids
        .iter()
        .filter_map(|id| solids.get(id).copied())
        .filter(|solid| solid.role == SolidRole::WallHost)
        .collect::<Vec<_>>();
    let half_length = wall.length_metres * 0.5;
    let mut expected = MultiPolygon(vec![timber_audit_polygon([
        Vec2::new(-half_length, 0.0),
        Vec2::new(half_length, 0.0),
        Vec2::new(half_length, wall.height_metres),
        Vec2::new(-half_length, wall.height_metres),
    ])]);
    for opening in plan
        .opening_assemblies
        .iter()
        .filter(|opening| opening.host_wall == wall.id)
    {
        let half_opening = (opening.profile.interior_width_metres() * 0.5).min(half_length - 0.02);
        let centre = (opening.frame.origin - wall.frame.origin).dot(wall.frame.tangent);
        let sill = (opening.sill_elevation_metres - wall.base_elevation_metres)
            .clamp(0.0, wall.height_metres);
        let head = (sill + opening.profile.clear_height_metres()).clamp(sill, wall.height_metres);
        expected = expected.difference(&timber_audit_polygon([
            Vec2::new(centre - half_opening, sill),
            Vec2::new(centre + half_opening, sill),
            Vec2::new(centre + half_opening, head),
            Vec2::new(centre - half_opening, head),
        ]));
    }
    let wall_member_ids = frame
        .bays
        .iter()
        .filter(|candidate| candidate.wall == Some(wall.id))
        .flat_map(|candidate| candidate.member_ids.iter().copied())
        .collect::<std::collections::HashSet<_>>();
    for member in frame
        .members
        .iter()
        .filter(|member| wall_member_ids.contains(&member.id))
    {
        expected = expected.difference(&timber_member_audit_polygon(member, wall));
    }
    let panel_polygons = panels
        .iter()
        .filter_map(|panel| timber_panel_audit_polygon(panel, wall))
        .collect::<Vec<_>>();
    let panel_union =
        panel_polygons
            .iter()
            .cloned()
            .fold(MultiPolygon(Vec::new()), |union, panel| {
                if union.0.is_empty() {
                    MultiPolygon(vec![panel])
                } else {
                    union.union(&panel)
                }
            });
    let expected_area = expected.unsigned_area();
    let panel_area_sum = panel_polygons
        .iter()
        .map(Polygon::unsigned_area)
        .sum::<f32>();
    let union_area = panel_union.unsigned_area();
    declared == authoritative
        && panels.len() == panel_polygons.len()
        && !panels.is_empty()
        && expected_area > 0.02
        && expected.difference(&panel_union).unsigned_area() <= 0.0005
        && panel_union.difference(&expected).unsigned_area() <= 0.0005
        && (panel_area_sum - union_area).max(0.0) <= 0.0005
}
