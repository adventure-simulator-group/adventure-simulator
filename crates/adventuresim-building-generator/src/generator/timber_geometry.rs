fn closed_polygon(points: impl IntoIterator<Item = Vec2>) -> Polygon<f32> {
    let mut coords = points
        .into_iter()
        .map(|point| Coord {
            x: point.x,
            y: point.y,
        })
        .collect::<Vec<_>>();
    if coords.first() != coords.last() {
        coords.push(coords[0]);
    }
    Polygon::new(LineString::new(coords), Vec::new())
}

fn timber_member_end_face_polygon(start: Vec2, end: Vec2, half: f32) -> Polygon<f32> {
    let axis = (end - start).normalize_or_zero();
    let normal = Vec2::new(-axis.y, axis.x);
    // `start` and `end` are the centres of the member's end faces. Extending
    // the subtraction along `axis` by another half section cuts unsupported
    // wedges out of the infill at every timber joint because the rendered
    // cuboid does not extend beyond those faces.
    closed_polygon([
        start - normal * half,
        end - normal * half,
        end + normal * half,
        start + normal * half,
    ])
}

fn timber_member_wall_polygon(
    member: &crate::TimberFrameMember,
    wall: &crate::WallAssembly,
) -> Polygon<f32> {
    let project = |point: Vec3| {
        Vec2::new(
            (Vec2::new(point.x, point.z) - wall.frame.origin).dot(wall.frame.tangent),
            point.y - wall.base_elevation_metres,
        )
    };
    timber_member_end_face_polygon(
        project(member.start),
        project(member.end),
        timber_infill_cut_half_width(member.section_metres),
    )
}

fn timber_infill_cut_half_width(section_metres: Vec2) -> f32 {
    (section_metres.max_element() * 0.5 - crate::TIMBER_INFILL_EDGE_UNDERLAP_METRES).max(0.0)
}

fn timber_infill_panel_depth(wall: &crate::WallAssembly) -> f32 {
    (wall.thickness_metres - crate::TIMBER_INFILL_FINISH_SETBACK_METRES).max(0.04)
}

fn timber_infill_mid_plane(wall: &crate::WallAssembly) -> Vec2 {
    wall.frame.origin - wall.frame.outward * (crate::TIMBER_INFILL_FINISH_SETBACK_METRES * 0.5)
}
