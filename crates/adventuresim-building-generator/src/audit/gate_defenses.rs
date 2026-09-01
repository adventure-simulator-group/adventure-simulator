fn audit_gate_defenses(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    for (wall_index, wall) in plan.curtain_walls.iter().enumerate() {
        if wall.gate_width_metres.is_none() {
            continue;
        }
        let Some(defense) = plan
            .gate_defenses
            .iter()
            .find(|defense| defense.curtain_wall_index == wall_index)
        else {
            issues.push(issue(
                "undefended_gate",
                format!("curtain gate {wall_index} has no declared defense"),
            ));
            continue;
        };
        let mut independent = std::collections::BTreeSet::new();
        let sectors = defense
            .firing_positions
            .iter()
            .filter(|position| {
                position.aperture_width_metres > 0.0
                    && position.aperture_width_metres <= 0.2
                    && firing_origin_matches_aperture(plan, position)
                    && firing_sector_covers(position, defense.threshold)
                    && firing_sector_covers(position, defense.approach)
                    && ray_clear_of_solids(plan, position, defense.threshold, wall_index)
                    && ray_clear_of_solids(plan, position, defense.approach, wall_index)
                    && independent.insert((position.tower_index, position.aperture_id))
            })
            .count();
        if sectors < 2 {
            issues.push(issue(
                "undefended_gate",
                format!("curtain gate {wall_index} has only {sectors} valid firing sectors"),
            ));
        }
        let heavy = defense
            .closures
            .iter()
            .any(|closure| closure.kind == GateClosureKind::HeavyLeaves);
        let second = defense
            .closures
            .iter()
            .any(|closure| closure.kind == GateClosureKind::Portcullis);
        if !heavy || !second {
            issues.push(issue(
                "undefended_gate",
                format!("curtain gate {wall_index} lacks two closures"),
            ));
        }
        if defense
            .closures
            .iter()
            .any(|closure| !closure_covers_passage(*closure, defense.passage_profile))
        {
            issues.push(issue(
                "unsealed_gate_passage",
                format!("curtain gate {wall_index} has a closure that leaves part of its arch profile open"),
            ));
        }
        audit_guard_chamber(plan, defense, wall_index, issues);
    }
}

fn closure_covers_passage(closure: crate::GateClosure, passage: crate::GatePassageProfile) -> bool {
    if closure.coverage.width_metres + 0.01 < passage.width_metres {
        return false;
    }
    (0..=16).all(|sample| {
        let along = (sample as f32 / 16.0 - 0.5) * passage.width_metres;
        closure.coverage.height_at(along) + 0.01 >= passage.height_at(along)
    })
}
