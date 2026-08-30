fn audit_fortified_profile(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    if !matches!(
        plan.archetype,
        BuildingArchetype::CourtyardCastle | BuildingArchetype::WalledKeep
    ) {
        return;
    }
    // These are declared game-profile gates for the modernized walled keep,
    // not universal measurements claimed for every sixteenth-century work.
    const MIN_MASONRY_THICKNESS: f32 = 1.2;
    const MAX_UNFLANKED_RUN: f32 = 36.0;

    for (index, wall) in plan.curtain_walls.iter().enumerate() {
        if wall.thickness_metres + 0.001 < MIN_MASONRY_THICKNESS {
            issues.push(issue(
                "wall_too_thin_for_profile",
                format!(
                    "curtain wall {index} is only {:.2} m thick",
                    wall.thickness_metres
                ),
            ));
        }
        let delta = wall.end - wall.start;
        let length = delta.length();
        let axis = delta / length.max(0.001);
        let mut positions = plan
            .towers
            .iter()
            .filter_map(|tower| {
                let projected = (tower.centre_metres() - wall.start).dot(axis);
                let nearest = wall.start + axis * projected.clamp(0.0, length);
                ((tower.centre_metres() - nearest).length() <= tower.radius_metres() + 0.25)
                    .then_some(projected.clamp(0.0, length))
            })
            .collect::<Vec<_>>();
        positions.extend([0.0, length]);
        positions.sort_by(f32::total_cmp);
        if positions
            .windows(2)
            .any(|pair| pair[1] - pair[0] > MAX_UNFLANKED_RUN)
        {
            issues.push(issue(
                "unflanked_curtain",
                format!("curtain wall {index} exceeds the declared flanking interval"),
            ));
        }
    }

    for (index, tower) in plan.towers.iter().enumerate() {
        if tower.wall_thickness_metres + 0.001 < MIN_MASONRY_THICKNESS {
            issues.push(issue(
                "wall_too_thin_for_profile",
                format!(
                    "tower {index} shell is only {:.2} m thick",
                    tower.wall_thickness_metres
                ),
            ));
        }
        let interior_radius = tower.radius_metres() - tower.wall_thickness_metres;
        if plan.stairs.iter().any(|stair| matches!(
            stair,
            Stair::Spiral { centre, outer_radius_metres, .. }
                if close_vec(*centre, tower.centre_metres()) && *outer_radius_metres > interior_radius - 0.1
        )) {
            issues.push(issue(
                "insufficient_walk_clearance",
                format!("tower {index} stair collides with its masonry shell"),
            ));
        }
    }
    if plan.archetype == BuildingArchetype::WalledKeep {
        audit_gate_defenses(plan, issues);
    }
}
