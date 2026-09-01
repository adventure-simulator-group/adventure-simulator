fn audit_gatehouse_assemblies(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    for (assembly_index, spec) in plan.gatehouse_assemblies.iter().copied().enumerate() {
        let Some(wall) = plan.curtain_walls.get(spec.curtain_wall_index).copied() else {
            issues.push(issue(
                "gatehouse_spec_drift",
                format!("gatehouse {assembly_index} references a missing curtain"),
            ));
            continue;
        };
        let Some(defense) = plan
            .gate_defenses
            .iter()
            .find(|defense| defense.curtain_wall_index == spec.curtain_wall_index)
        else {
            issues.push(issue(
                "gatehouse_spec_drift",
                format!("gatehouse {assembly_index} has no resolved defense"),
            ));
            continue;
        };
        let tangent = (wall.end - wall.start).normalize_or_zero();
        let outward = direction_vector(wall.outward);
        if !(tangent.x.abs() >= 0.999 || tangent.y.abs() >= 0.999)
            || tangent.dot(outward).abs() > 0.001
        {
            issues.push(issue(
                "invalid_gatehouse_orientation",
                format!(
                    "gatehouse {assembly_index} requires a cardinal wall and perpendicular outward"
                ),
            ));
            continue;
        }
        let threshold = (wall.start + wall.end) * 0.5;
        if wall
            .gate_width_metres
            .is_none_or(|width| (width - spec.gate_width.metres()).abs() > 0.01)
        {
            issues.push(issue(
                "gatehouse_spec_drift",
                format!("gatehouse {assembly_index} curtain opening differs from its source spec"),
            ));
        }
        if (defense.passage_profile.width_metres - spec.gate_width.metres()).abs() > 0.01
            || (defense.passage_profile.spring_height_metres - wall.gate_height_metres).abs() > 0.01
            || (defense.passage_profile.arch_rise_metres - spec.arch_rise.metres()).abs() > 0.01
        {
            issues.push(issue(
                "gatehouse_spec_drift",
                format!(
                    "gatehouse {assembly_index} passage cross-section differs from its source spec"
                ),
            ));
        }
        let radius = spec.tower_diameter.metres() * 0.5;
        let tower_offset = spec.gate_width.metres() * 0.5 + spec.jamb_reveal.metres() + radius;
        let expected_centres = [
            threshold - tangent * tower_offset,
            threshold + tangent * tower_offset,
        ];
        let crate::GatehouseLoadPath::BondedTowerBearing {
            left_tower_index,
            right_tower_index,
            bearing_depth,
            arch_centre,
            arch_spring_elevation_metres,
            arch_ring_depth,
            arch_rise,
            curtain_return_bond,
        } = defense.guard_chamber.load_path;
        let tower_indices = [left_tower_index, right_tower_index];
        let mut resolved = [None, None];
        for (side, (&index, expected)) in tower_indices.iter().zip(expected_centres).enumerate() {
            resolved[side] = plan.towers.get(index).copied();
            let Some(tower) = resolved[side] else {
                issues.push(issue(
                    "declared_load_path",
                    format!("gatehouse {assembly_index} bearing references missing tower {index}"),
                ));
                continue;
            };
            if tower.diameter() != spec.tower_diameter
                || (tower.centre_metres() - expected).length() > 0.01
                || (tower.wall_thickness_metres - spec.tower_shell.metres()).abs() > 0.01
            {
                issues.push(issue("gatehouse_spec_drift", format!("gatehouse {assembly_index} tower {index} is not derived from its discrete anchor/diameter")));
            }
            let expected_direction = if side == 0 {
                cardinal_direction(tangent)
            } else {
                cardinal_direction(-tangent)
            };
            if !tower.chord_interface.is_some_and(|interface| {
                interface.toward_gate == expected_direction
                    && interface.bearing_depth == spec.chord_bearing
            }) {
                issues.push(issue(
                    "round_rect_splice",
                    format!(
                        "gatehouse {assembly_index} tower {index} lacks its derived chord interface"
                    ),
                ));
            }
        }
        if bearing_depth != spec.chord_bearing
            || arch_ring_depth != spec.arch_ring_depth
            || arch_rise != spec.arch_rise
            || curtain_return_bond != spec.curtain_return_bond
            || (arch_centre - threshold).length() > 0.01
            || (arch_spring_elevation_metres - wall.gate_height_metres).abs() > 0.01
        {
            issues.push(issue(
                "declared_load_path",
                format!("gatehouse {assembly_index} load path differs from its source spec"),
            ));
        }

        let chamber = &defense.guard_chamber;
        let chamber_along = chamber.size.dot(tangent.abs());
        let chamber_depth = chamber.size.dot(outward.abs());
        let expected_along = 2.0 * (tower_offset - (radius - spec.chord_bearing.metres()));
        if (chamber.centre - threshold).length() > 0.01
            || (chamber_along - expected_along).abs() > 0.01
            || (chamber_depth - spec.chamber_depth.metres()).abs() > 0.01
        {
            issues.push(issue(
                "gatehouse_spec_drift",
                format!("gatehouse {assembly_index} chamber is not the wall-local derived volume"),
            ));
        }
        let access = &chamber.access;
        let expected_depth = spec.chamber_depth.metres() * 0.5 + 0.6;
        let expected_top = threshold - tangent * 1.9 + (-outward) * expected_depth;
        let expected_bottom = threshold + tangent * 1.9 + (-outward) * expected_depth;
        let expected_door =
            threshold + tangent * 1.9 + (-outward) * (spec.chamber_depth.metres() * 0.5);
        if (access.top_landing.centre - expected_top).length() > 0.01
            || (access.bottom_landing.centre - expected_bottom).length() > 0.01
            || (access.door.position - expected_door).length() > 0.01
            || (access.flight.top - (expected_top + tangent * 0.5)).length() > 0.01
            || (access.flight.bottom - (expected_bottom - tangent * 0.5)).length() > 0.01
        {
            issues.push(issue(
                "gatehouse_spec_drift",
                format!("gatehouse {assembly_index} access route is not derived wall-locally"),
            ));
        }
        let supported_half_span =
            spec.gate_width.metres() * 0.5 + spec.jamb_reveal.metres() + bearing_depth.metres();
        if chamber_along * 0.5 > supported_half_span + 0.01 {
            issues.push(issue(
                "declared_load_path",
                format!("gatehouse {assembly_index} floor projection exceeds its arch-and-tower tributary support"),
            ));
        }
        let chord_half = (radius * radius - (radius - spec.chord_bearing.metres()).powi(2)).sqrt();
        if (chord_half - chamber_depth * 0.5).abs() > 0.02 {
            issues.push(issue(
                "round_rect_splice",
                format!("gatehouse {assembly_index} chamber edge does not match its tower chord"),
            ));
        }
        let arch_bottom = arch_spring_elevation_metres;
        let arch_top = arch_bottom + arch_ring_depth.metres() + arch_rise.metres();
        let floor_bottom = chamber.floor_elevation_metres - 0.09;
        if arch_bottom + 0.01 < wall.gate_height_metres || floor_bottom + 0.01 < arch_top {
            issues.push(issue(
                "gate_passage_clear",
                format!("gatehouse {assembly_index} arch/floor intrudes into the required passage"),
            ));
        }
        let required_return = radius
            - (radius * radius - (wall.thickness_metres * 0.5).powi(2))
                .max(0.0)
                .sqrt();
        if curtain_return_bond.metres() + 0.001 < required_return
            || curtain_return_bond.metres() > spec.tower_shell.metres()
        {
            issues.push(issue(
                "round_rect_splice",
                format!("gatehouse {assembly_index} curtain return lacks a positive full-face tower bond"),
            ));
        }
        if chamber.supports.iter().any(|support| {
            let local = support.centre - threshold;
            let along = local.dot(tangent).abs();
            let half_along = support.size.dot(tangent.abs()) * 0.5;
            along - half_along < spec.gate_width.metres() * 0.5
                && support.base_elevation_metres < wall.gate_height_metres
        }) {
            issues.push(issue(
                "gate_passage_clear",
                format!("gatehouse {assembly_index} has a solid in its required passage void"),
            ));
        }
        if chamber.floor_elevation_metres < wall.gate_height_metres
            || chamber
                .openings
                .iter()
                .any(|opening| opening.width_metres < 0.1 || opening.clear_height_metres < 0.1)
        {
            issues.push(issue("room_void_disjoint_from_solids", format!("gatehouse {assembly_index} usable chamber/opening volume intersects resolved solids")));
        }
        let room_rect = oriented_rect(
            chamber.centre,
            tangent,
            outward,
            (chamber_along * 0.5 - 0.28).max(0.0),
            (chamber_depth * 0.5 - 0.28).max(0.0),
        );
        let passage_rect = oriented_rect(
            threshold,
            tangent,
            outward,
            spec.gate_width.metres() * 0.5,
            chamber_depth * 0.5,
        );
        let room_prism = Prism {
            rect: room_rect,
            low: chamber.floor_elevation_metres + 0.09,
            high: chamber.floor_elevation_metres + chamber.clear_height_metres,
        };
        let passage_prism = Prism {
            rect: passage_rect,
            low: 0.0,
            high: wall.gate_height_metres,
        };
        for tower in resolved.into_iter().flatten() {
            if retained_tower_overlaps_rect(tower, room_prism.rect) {
                issues.push(issue(
                    "room_void_disjoint_from_solids",
                    format!(
                        "gatehouse {assembly_index} chamber clear prism intersects a tower solid"
                    ),
                ));
                issues.push(issue(
                    "undeclared_solid_overlap",
                    format!("gatehouse {assembly_index} chamber crosses its declared tower chord"),
                ));
            }
            if circle_overlaps_rect(
                tower.centre_metres(),
                tower.radius_metres(),
                passage_prism.rect,
            ) {
                issues.push(issue(
                    "gate_passage_clear",
                    format!("gatehouse {assembly_index} tower intrudes into the passage prism"),
                ));
            }
        }
        for support in &chamber.supports {
            let support_prism = Prism {
                rect: axis_rect(support.centre, support.size * 0.5),
                low: support.base_elevation_metres,
                high: support.top_elevation_metres,
            };
            if prisms_overlap(support_prism, passage_prism) {
                issues.push(issue(
                    "gate_passage_clear",
                    format!("gatehouse {assembly_index} support intersects the passage prism"),
                ));
            }
            if prisms_overlap(support_prism, room_prism) {
                issues.push(issue(
                    "room_void_disjoint_from_solids",
                    format!(
                        "gatehouse {assembly_index} support intersects the chamber clear prism"
                    ),
                ));
            }
            if resolved.into_iter().flatten().any(|tower| {
                circle_overlaps_rect(
                    tower.centre_metres(),
                    tower.radius_metres(),
                    support_prism.rect,
                )
            }) {
                issues.push(issue(
                    "undeclared_solid_overlap",
                    format!("gatehouse {assembly_index} support overlaps a tower outside a declared bearing"),
                ));
            }
        }
        for (index, support) in chamber.supports.iter().enumerate() {
            let a = Prism {
                rect: axis_rect(support.centre, support.size * 0.5),
                low: support.base_elevation_metres,
                high: support.top_elevation_metres,
            };
            for other in chamber.supports.iter().skip(index + 1) {
                let b = Prism {
                    rect: axis_rect(other.centre, other.size * 0.5),
                    low: other.base_elevation_metres,
                    high: other.top_elevation_metres,
                };
                if prisms_overlap(a, b) {
                    issues.push(issue(
                        "undeclared_solid_overlap",
                        format!("gatehouse {assembly_index} supports overlap with positive volume"),
                    ));
                }
            }
        }
        if let [Some(left), Some(right)] = resolved
            && (left.centre_metres() - right.centre_metres()).length()
                < left.radius_metres() + right.radius_metres() - 0.001
        {
            issues.push(issue(
                "undeclared_solid_overlap",
                format!("gatehouse {assembly_index} flanking towers overlap"),
            ));
        }
        if chamber_depth > chord_half * 2.0 + 0.02 {
            issues.push(issue(
                "room_void_disjoint_from_solids",
                format!(
                    "gatehouse {assembly_index} chamber side walls exceed the open tower chords"
                ),
            ));
        }
        // A 256-segment tower shell must omit at least two facets across every
        // firing slit, otherwise the semantic aperture would render closed.
        let shell_sample = std::f32::consts::TAU * radius / 256.0;
        if defense.firing_positions.iter().any(|position| {
            !tower_indices.contains(&position.tower_index)
                || position.aperture_width_metres + 0.001 < shell_sample * 2.0
        }) {
            issues.push(issue(
                "aperture_clearance",
                format!(
                    "gatehouse {assembly_index} firing aperture is not resolved by the tower shell"
                ),
            ));
        }
        // The independent curtain renderer is required to terminate at the
        // outer tower tangencies; a tower located away from the wall axis would
        // make that trim overlap or leave an undeclared gap.
        if resolved
            .into_iter()
            .flatten()
            .any(|tower| ((tower.centre_metres() - threshold).dot(outward)).abs() > 0.01)
        {
            issues.push(issue("undeclared_solid_overlap", format!("gatehouse {assembly_index} tower is not coplanar with the resolved curtain splice")));
        }
    }
}
