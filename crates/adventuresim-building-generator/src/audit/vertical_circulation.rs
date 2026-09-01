fn audit_vertical_circulation(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    for (index, stair) in plan.stairs.iter().copied().enumerate() {
        let Stair::Straight {
            start,
            direction,
            base_height_metres,
            rise_metres,
            run_metres,
            ..
        } = stair
        else {
            continue;
        };
        let axis = direction_vector(direction);
        let lateral = Vec2::new(-axis.y, axis.x);
        let base_level = (base_height_metres / plan.storey_height_metres).round() as u16;
        audit_stair_landing(plan, index, start, base_level, axis, lateral, issues);
        let arrival_height = base_height_metres + rise_metres;
        let arrival_level = (arrival_height / plan.storey_height_metres).round() as u16;
        audit_stair_landing(
            plan,
            index,
            start + axis * run_metres,
            arrival_level,
            axis,
            lateral,
            issues,
        );
    }
}

fn audit_stair_landing(
    plan: &BuildingPlan,
    stair_index: usize,
    landing: Vec2,
    level: u16,
    axis: Vec2,
    lateral: Vec2,
    issues: &mut Vec<AuditIssue>,
) {
    let Some(storey) = plan.storeys.iter().find(|storey| storey.level == level) else {
        issues.push(issue(
            "invalid_vertical_circulation",
            format!("straight stair {stair_index} reaches missing storey {level}"),
        ));
        return;
    };
    let Some(stair_hall) = storey.rooms.iter().find(|room| {
        room.kind == RoomKind::StairHall
            && room.cells.iter().any(|cell| {
                let min = cell.centre() - Vec2::splat(crate::CELL_SIZE_METRES * 0.5);
                let max = cell.centre() + Vec2::splat(crate::CELL_SIZE_METRES * 0.5);
                landing.cmpge(min).all() && landing.cmple(max).all()
            })
    }) else {
        issues.push(issue(
            "invalid_vertical_circulation",
            format!("straight stair {stair_index} does not meet a stair hall on storey {level}"),
        ));
        return;
    };
    let landing_envelope_is_inside = [-0.45_f32, 0.0, 0.45].into_iter().all(|across| {
        [-0.45_f32, 0.0, 0.45].into_iter().all(|along| {
            let sample = landing + lateral * across + axis * along;
            stair_hall.cells.iter().any(|cell| {
                let min = cell.centre() - Vec2::splat(crate::CELL_SIZE_METRES * 0.5 + 0.001);
                let max = cell.centre() + Vec2::splat(crate::CELL_SIZE_METRES * 0.5 + 0.001);
                sample.cmpge(min).all() && sample.cmple(max).all()
            })
        })
    });
    let has_room_door = storey.openings.iter().any(|opening| {
        if opening.kind != OpeningKind::Door {
            return false;
        }
        let Some(wall) = storey.walls.get(opening.wall).copied() else {
            return false;
        };
        wall.outside_room.is_some()
            && (wall.inside_room == stair_hall.id || wall.outside_room == Some(stair_hall.id))
    });
    if !landing_envelope_is_inside || !has_room_door {
        issues.push(issue(
            "invalid_vertical_circulation",
            format!(
                "straight stair {stair_index} lacks a 0.90 m clear landing and room-graph doorway on storey {level}"
            ),
        ));
    }
}
