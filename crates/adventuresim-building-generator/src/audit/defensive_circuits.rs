fn audit_defensive_circuit(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    if plan.wall_walks.is_empty() {
        return;
    }
    let mut graph = vec![Vec::new(); plan.wall_walks.len()];
    for (index, junction) in plan.defensive_junctions.iter().enumerate() {
        let Some(a) = plan.wall_walks.get(junction.walk_a).copied() else {
            issues.push(issue(
                "disconnected_defensive_circuit",
                format!("defensive junction {index} references a missing walk"),
            ));
            continue;
        };
        let Some(b) = plan.wall_walks.get(junction.walk_b).copied() else {
            issues.push(issue(
                "disconnected_defensive_circuit",
                format!("defensive junction {index} references a missing walk"),
            ));
            continue;
        };
        let delta = (walk_elevation(a) - walk_elevation(b)).abs();
        match junction.kind {
            DefensiveJunctionKind::LevelLanding if delta > 0.2 => issues.push(issue(
                "wall_walk_vertical_discontinuity",
                format!("level junction {index} bridges a {delta:.2} m height difference"),
            )),
            DefensiveJunctionKind::Steps { riser_count }
                if riser_count == 0 || delta / f32::from(riser_count) > 0.20 =>
            {
                issues.push(issue(
                    "wall_walk_vertical_discontinuity",
                    format!("stepped junction {index} has unusable risers"),
                ));
            }
            _ => {}
        }
        if junction.width_metres < 0.9 || junction.clear_height_metres < 1.9 {
            issues.push(issue(
                "insufficient_walk_clearance",
                format!(
                    "defensive junction {index} has {:.2} m width and {:.2} m headroom",
                    junction.width_metres, junction.clear_height_metres
                ),
            ));
        }
        if !junction_has_physical_connection(plan, junction, a, b) {
            issues.push(issue(
                "missing_tower_portal",
                format!("defensive junction {index} is not backed by a physical portal/landing"),
            ));
            continue;
        }
        graph[junction.walk_a].push(junction.walk_b);
        graph[junction.walk_b].push(junction.walk_a);
    }

    let mut assignments = vec![0_u8; plan.wall_walks.len()];
    for circuit in &plan.defensive_circuits {
        audit_one_defensive_circuit(plan, circuit, &graph, &mut assignments, issues);
    }
    for (index, count) in assignments.into_iter().enumerate() {
        if count != 1 {
            issues.push(issue(
                "disconnected_defensive_circuit",
                format!("wall walk {index} belongs to {count} declared circuits instead of one"),
            ));
        }
    }
    for (tower_index, tower) in plan.towers.iter().enumerate() {
        if !plan.tower_portals.iter().any(|portal| {
            portal.tower_index == tower_index
                && portal.kind == TowerPortalKind::GroundStairEntrance
                && portal.width_metres >= 0.9
                && portal.clear_height_metres >= 1.9
                && portal.sill_elevation_metres <= 0.2
        }) {
            issues.push(issue(
                "missing_tower_portal",
                format!("tower {tower_index} has no usable ground stair entrance"),
            ));
        }
        if tower.radius_metres() - tower.wall_thickness_metres < 0.9 {
            issues.push(issue(
                "insufficient_walk_clearance",
                format!("tower {tower_index} has less than 0.90 m interior radius"),
            ));
        }
    }
}

fn junction_has_physical_connection(
    plan: &BuildingPlan,
    junction: &DefensiveJunction,
    a: WallWalk,
    b: WallWalk,
) -> bool {
    let (linear_index, round) = match (a, b) {
        (WallWalk::Linear { .. }, round @ WallWalk::Round { .. }) => (junction.walk_a, round),
        (round @ WallWalk::Round { .. }, WallWalk::Linear { .. }) => (junction.walk_b, round),
        _ => return true,
    };
    let WallWalk::Round { centre, .. } = round else {
        unreachable!()
    };
    let Some(tower_index) = plan
        .towers
        .iter()
        .position(|tower| close_vec(tower.centre_metres(), centre))
    else {
        return false;
    };
    plan.tower_portals.iter().any(|portal| {
        portal.tower_index == tower_index
            && portal.kind
                == (TowerPortalKind::WallWalkJunction {
                    walk_index: linear_index,
                })
            && portal.width_metres >= junction.width_metres
            && portal.clear_height_metres >= junction.clear_height_metres
    })
}

fn audit_one_defensive_circuit(
    plan: &BuildingPlan,
    circuit: &DefensiveCircuit,
    graph: &[Vec<usize>],
    assignments: &mut [u8],
    issues: &mut Vec<AuditIssue>,
) {
    let mut member = vec![false; plan.wall_walks.len()];
    for &index in &circuit.walks {
        let Some(assignment) = assignments.get_mut(index) else {
            issues.push(issue(
                "disconnected_defensive_circuit",
                format!("{} references missing wall walk {index}", circuit.label),
            ));
            continue;
        };
        *assignment = assignment.saturating_add(1);
        member[index] = true;
    }
    let Some(seed) = circuit.walks.iter().copied().find(|&index| {
        plan.wall_walks
            .get(index)
            .is_some_and(|walk| walk_has_stair_access(plan, *walk))
    }) else {
        issues.push(issue(
            "disconnected_defensive_circuit",
            format!("{} has no interior stair access", circuit.label),
        ));
        return;
    };
    let mut reachable = vec![false; plan.wall_walks.len()];
    let mut queue = VecDeque::from([seed]);
    reachable[seed] = true;
    while let Some(current) = queue.pop_front() {
        for &next in &graph[current] {
            if member[next] && !reachable[next] {
                reachable[next] = true;
                queue.push_back(next);
            }
        }
    }
    for &index in &circuit.walks {
        if index < reachable.len() && !reachable[index] {
            issues.push(issue(
                "disconnected_defensive_circuit",
                format!("wall walk {index} is disconnected within {}", circuit.label),
            ));
        }
    }
}

fn walk_has_stair_access(plan: &BuildingPlan, walk: WallWalk) -> bool {
    match walk {
        WallWalk::Round {
            centre,
            elevation_metres,
            ..
        } => plan.stairs.iter().any(|stair| {
            matches!(
                stair,
                Stair::Spiral { centre: stair_centre, base_height_metres, rise_metres, .. }
                    if close_vec(*stair_centre, centre)
                        && close(*base_height_metres + *rise_metres, elevation_metres)
            )
        }),
        WallWalk::RectangularDeck {
            stairwell_centre,
            elevation_metres,
            ..
        } => plan.stairs.iter().any(|stair| {
            matches!(
                stair,
                Stair::Spiral { centre, base_height_metres, rise_metres, .. }
                    if close_vec(*centre, stairwell_centre)
                        && close(*base_height_metres + *rise_metres, elevation_metres)
            )
        }),
        WallWalk::Linear { .. } => false,
    }
}
