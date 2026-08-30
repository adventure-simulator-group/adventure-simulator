fn cardinal_direction(vector: Vec2) -> Direction {
    if vector.x.abs() >= vector.y.abs() {
        if vector.x >= 0.0 {
            Direction::East
        } else {
            Direction::West
        }
    } else if vector.y >= 0.0 {
        Direction::North
    } else {
        Direction::South
    }
}

#[derive(Clone, Copy)]
struct Rect2 {
    min: Vec2,
    max: Vec2,
}

#[derive(Clone, Copy)]
struct Prism {
    rect: Rect2,
    low: f32,
    high: f32,
}

fn axis_rect(centre: Vec2, half: Vec2) -> Rect2 {
    Rect2 {
        min: centre - half,
        max: centre + half,
    }
}

fn oriented_rect(
    centre: Vec2,
    tangent: Vec2,
    outward: Vec2,
    half_along: f32,
    half_depth: f32,
) -> Rect2 {
    let half = tangent.abs() * half_along + outward.abs() * half_depth;
    axis_rect(centre, half)
}

fn prisms_overlap(a: Prism, b: Prism) -> bool {
    a.low < b.high - 0.001 && a.high > b.low + 0.001 && rects_overlap(a.rect, b.rect)
}

fn rects_overlap(a: Rect2, b: Rect2) -> bool {
    a.min.x < b.max.x - 0.001
        && a.max.x > b.min.x + 0.001
        && a.min.y < b.max.y - 0.001
        && a.max.y > b.min.y + 0.001
}

fn circle_overlaps_rect(centre: Vec2, radius: f32, rect: Rect2) -> bool {
    let nearest = centre.clamp(rect.min, rect.max);
    (nearest - centre).length_squared() < (radius - 0.001).powi(2)
}

fn retained_tower_overlaps_rect(tower: crate::RoundTower, mut rect: Rect2) -> bool {
    let centre = tower.centre_metres();
    for interface in tower.chord_interfaces() {
        let cut = tower.radius_metres() - interface.bearing_depth.metres();
        match interface.toward_gate {
            Direction::East => rect.max.x = rect.max.x.min(centre.x + cut),
            Direction::West => rect.min.x = rect.min.x.max(centre.x - cut),
            Direction::North => rect.max.y = rect.max.y.min(centre.y + cut),
            Direction::South => rect.min.y = rect.min.y.max(centre.y - cut),
        }
    }
    rect.min.x < rect.max.x - 0.001
        && rect.min.y < rect.max.y - 0.001
        && circle_overlaps_rect(centre, tower.radius_metres(), rect)
}
