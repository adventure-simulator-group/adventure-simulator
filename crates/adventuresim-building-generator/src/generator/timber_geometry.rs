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
