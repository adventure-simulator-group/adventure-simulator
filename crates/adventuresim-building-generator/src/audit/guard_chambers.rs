fn audit_guard_chamber(
    plan: &BuildingPlan,
    defense: &crate::GateDefense,
    wall_index: usize,
    issues: &mut Vec<AuditIssue>,
) {
    let chamber = &defense.guard_chamber;
    let Some(wall) = plan.curtain_walls.get(chamber.supporting_wall_index) else {
        issues.push(issue(
            "unsupported_guard_chamber",
            format!("gate chamber for wall {wall_index} references a missing support wall"),
        ));
        return;
    };
    let wall_tangent = (wall.end - wall.start).normalize_or_zero();
    let inward = -direction_vector(wall.outward);
    let gate_half_width = wall.gate_width_metres.unwrap_or(0.0) * 0.5;
    let chamber_half = chamber.size * 0.5;
    let supports_are_piers = !chamber.supports.is_empty()
        && chamber.supports.iter().all(|support| {
            let relative = support.centre - chamber.centre;
            let tangent_offset = relative.dot(wall_tangent);
            let inward_offset = relative.dot(inward);
            let tangent_half_extent = support.size.x * wall_tangent.x.abs() * 0.5
                + support.size.y * wall_tangent.y.abs() * 0.5;
            let inward_half_extent =
                support.size.x * inward.x.abs() * 0.5 + support.size.y * inward.y.abs() * 0.5;
            tangent_offset.abs() + tangent_half_extent <= chamber_half.x.max(chamber_half.y) + 0.05
                && inward_offset.abs() + inward_half_extent
                    <= chamber_half.x.max(chamber_half.y) + 0.05
                && (support.centre - defense.threshold).dot(wall_tangent).abs()
                    - tangent_half_extent
                    >= gate_half_width - 0.05
        });
    let bonded_load_path = match chamber.load_path {
        crate::GatehouseLoadPath::BondedTowerBearing {
            left_tower_index,
            right_tower_index,
            bearing_depth,
            arch_ring_depth,
            arch_rise,
            curtain_return_bond,
            ..
        } => {
            let towers =
                [left_tower_index, right_tower_index].map(|index| plan.towers.get(index).copied());
            towers.iter().all(Option::is_some)
                && towers.into_iter().flatten().all(|tower| {
                    tower
                        .chord_interface
                        .is_some_and(|interface| interface.bearing_depth == bearing_depth)
                })
                && bearing_depth.units() > 0
                && arch_ring_depth.units() > 0
                && arch_rise.units() > 0
                && curtain_return_bond.units() > 0
        }
    };
    if chamber.supporting_wall_index != wall_index
        || chamber.size.x * chamber.size.y < 6.0
        || chamber.clear_height_metres < 2.0
        || chamber.floor_elevation_metres + 0.01 < wall.gate_height_metres
        || chamber.supports.iter().any(|support| {
            !close(support.top_elevation_metres, chamber.floor_elevation_metres)
                || support.base_elevation_metres > 0.2
        })
        || (!supports_are_piers && !bonded_load_path)
    {
        issues.push(issue(
            "unsupported_guard_chamber",
            format!("gate chamber for wall {wall_index} lacks supported usable volume"),
        ));
    }
    let access = &chamber.access;
    let access_walk = plan.wall_walks.get(access.from_walk_index).copied();
    let access_walk_is_in_reachable_circuit = plan.defensive_circuits.iter().any(|circuit| {
        circuit.walks.contains(&access.from_walk_index)
            && circuit.walks.iter().copied().any(|index| {
                plan.wall_walks
                    .get(index)
                    .copied()
                    .is_some_and(|walk| walk_has_stair_access(plan, walk))
            })
    });
    if !access_walk_is_in_reachable_circuit {
        issues.push(issue(
            "inaccessible_guard_chamber",
            format!("gate chamber for wall {wall_index} has no usable route from the wall walk"),
        ));
    }
    audit_guard_access(plan, defense, wall, access_walk, issues);
    let portcullis_position = chamber.operating_positions.iter().any(|position| {
        defense
            .closures
            .get(position.closure_index)
            .is_some_and(|closure| {
                closure.kind == GateClosureKind::Portcullis
                    && (position.position
                        - (defense.threshold + inward * closure.inward_offset_metres))
                        .length()
                        <= 0.4
            })
            && close(position.elevation_metres, chamber.floor_elevation_metres)
            && point_in_rect(position.position, chamber.centre, chamber.size)
    });
    let outward = chamber.openings.iter().any(|opening| {
        opening.kind == crate::GuardOpeningKind::OutwardObservation
            && opening.facing == wall.outward
            && opening.width_metres > 0.0
            && opening.clear_height_metres > 0.0
            && point_on_rect_boundary(opening.position, chamber.centre, chamber.size, 0.15)
    });
    let downward = chamber.openings.iter().any(|opening| {
        opening.kind == crate::GuardOpeningKind::DownwardDefense
            && (opening.target - defense.threshold).length() < 0.25
            && opening.width_metres > 0.0
            && opening.clear_height_metres > 0.0
            && point_in_rect(opening.position, chamber.centre, chamber.size)
    });
    if !portcullis_position || !outward || !downward {
        issues.push(issue(
            "inoperable_guard_chamber",
            format!("gate chamber for wall {wall_index} cannot observe and operate its defenses"),
        ));
    }
}
