#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuditIssue {
    pub code: &'static str,
    pub message: String,
}

pub fn audit_plan(plan: &BuildingPlan) -> Vec<AuditIssue> {
    let mut issues = Vec::new();
    let dimensions = plan.dimensions_metres();
    let centre = dimensions * 0.5;

    for (index, run) in plan.battlements.iter().enumerate() {
        let midpoint = (run.start + run.end) * 0.5;
        if (midpoint - centre).dot(direction_vector(run.outward)) <= 0.01 {
            issues.push(issue(
                "battlement_faces_inward",
                format!("battlement run {index} faces the protected interior"),
            ));
        }
        if run.kind != BattlementKind::Breteche
            && !run_supported(plan, run.start, run.end, run.base_height_metres)
        {
            issues.push(issue(
                "unsupported_battlement",
                format!("battlement run {index} has no wall beneath it"),
            ));
        }
        if run.kind != BattlementKind::Breteche
            && !plan.wall_walks.iter().any(|walk| match walk {
                WallWalk::Linear {
                    start,
                    end,
                    elevation_metres,
                    ..
                } => {
                    same_run(*start, *end, run.start, run.end)
                        && close(*elevation_metres, run.base_height_metres)
                }
                WallWalk::Round { .. } | WallWalk::RectangularDeck { .. } => false,
            })
        {
            issues.push(issue(
                "missing_wall_walk",
                format!("battlement run {index} has no fighting platform"),
            ));
        }
    }

    for (index, walk) in plan.wall_walks.iter().enumerate() {
        if let WallWalk::Linear { width_metres, .. } = walk
            && *width_metres < 0.9
        {
            issues.push(issue(
                "wall_walk_too_narrow",
                format!("wall walk {index} is only {width_metres:.2} m wide"),
            ));
        }
        if let WallWalk::RectangularDeck {
            stairwell_centre,
            stairwell_size,
            elevation_metres,
            ..
        } = walk
        {
            if stairwell_size.min_element() < 0.9 {
                issues.push(issue(
                    "stairwell_too_small",
                    format!("rectangular deck {index} has an undersized stairwell"),
                ));
            }
            if !plan.stairs.iter().any(|stair| {
                matches!(
                    stair,
                    Stair::Spiral { centre, base_height_metres, rise_metres, .. }
                        if close_vec(*centre, *stairwell_centre)
                            && close(*base_height_metres + *rise_metres, *elevation_metres)
                )
            }) {
                issues.push(issue(
                    "inaccessible_rectangular_deck",
                    format!("rectangular deck {index} has no stair reaching its stairwell"),
                ));
            }
        }
    }

    audit_defensive_circuit(plan, &mut issues);
    audit_resolved_geometry(plan, &mut issues);
    audit_wall_opening_assemblies(plan, &mut issues);
    audit_crowns(plan, &mut issues);
    audit_projected_defenses(plan, &mut issues);
    audit_roof_assemblies(plan, &mut issues);
    audit_church_assembly(plan, &mut issues);
    audit_timber_frame(plan, &mut issues);
    audit_vertical_circulation(plan, &mut issues);
    audit_artillery_castle(plan, &mut issues);

    if matches!(
        plan.archetype,
        BuildingArchetype::CourtyardCastle | BuildingArchetype::WalledKeep
    ) {
        audit_walk_roof_clearance(plan, &mut issues);
    }

    for (index, tower) in plan.towers.iter().enumerate() {
        if tower.battlement.is_some() && tower.roof.is_some() {
            issues.push(issue(
                "obstructed_tower_deck",
                format!("tower {index} has both a roof and an open fighting crown"),
            ));
        }
        let deck = plan.wall_walks.iter().any(|walk| {
            matches!(
                walk,
                WallWalk::Round { centre, elevation_metres, .. }
                    if close_vec(*centre, tower.centre_metres())
                        && close(*elevation_metres, tower.wall_height_metres)
            )
        });
        let access = plan.stairs.iter().any(|stair| {
            matches!(
                stair,
                Stair::Spiral { centre, base_height_metres, rise_metres, .. }
                    if close_vec(*centre, tower.centre_metres())
                        && close(*base_height_metres + *rise_metres, tower.wall_height_metres)
            )
        });
        if tower.battlement.is_some() && !deck {
            issues.push(issue(
                "missing_tower_deck",
                format!("battlemented tower {index} has no annular fighting deck"),
            ));
        }
        if tower.battlement.is_some() && !access {
            issues.push(issue(
                "inaccessible_tower_deck",
                format!("battlemented tower {index} has no stair reaching its deck"),
            ));
        }
    }

    for (index, wall) in plan.curtain_walls.iter().enumerate() {
        if !plan.battlements.iter().any(|run| {
            same_run(run.start, run.end, wall.start, wall.end)
                && close(run.base_height_metres, wall.height_metres)
                && run.outward == wall.outward
        }) {
            issues.push(issue(
                "unprotected_curtain_wall",
                format!("curtain wall {index} has no outward-facing parapet"),
            ));
        }
    }
    audit_fortified_profile(plan, &mut issues);
    audit_gatehouse_assemblies(plan, &mut issues);
    issues
}
