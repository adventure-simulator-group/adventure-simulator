fn derive_gate_defenses(
    _program: &BuildingProgram,
    gatehouses: &[GatehouseAssemblySpec],
    towers: &[RoundTower],
    curtain_walls: &[CurtainWallRun],
    wall_walks: &[WallWalk],
) -> Vec<GateDefense> {
    gatehouses
        .iter()
        .filter_map(|spec| {
            let wall_index = spec.curtain_wall_index;
            let wall = curtain_walls.get(wall_index)?;
            let threshold = (wall.start + wall.end) * 0.5;
            let outward = direction_vector(wall.outward);
            let inward = -outward;
            let tangent = (wall.end - wall.start).normalize_or_zero();
            let approach = threshold + outward * 6.0;
            let expected = resolve_gatehouse_towers(*spec, *wall, wall.height_metres)?;
            let tower_indices = expected
                .iter()
                .filter_map(|expected_tower| {
                    towers
                        .iter()
                        .position(|tower| tower.anchor() == expected_tower.anchor())
                })
                .collect::<Vec<_>>();
            if tower_indices.len() != 2 {
                return None;
            }
            let firing_positions = tower_indices
                .iter()
                .copied()
                .enumerate()
                .map(|(aperture_id, tower_index)| {
                    let tower = towers[tower_index];
                    let tower_centre = tower.centre_metres();
                    let aperture_normal = (threshold - tower_centre).normalize_or_zero();
                    let origin = tower_centre + aperture_normal * tower.radius_metres();
                    let direction = ((threshold - origin).normalize_or_zero()
                        + (approach - origin).normalize_or_zero())
                    .normalize_or_zero();
                    FiringPosition {
                        aperture_id: aperture_id as u16,
                        tower_index,
                        origin,
                        aperture_normal,
                        direction,
                        elevation_metres: 1.6,
                        range_metres: 24.0,
                        half_arc_degrees: 38.0,
                        aperture_width_metres: 0.18,
                    }
                })
                .collect();
            // The chamber floor bears above the crown of the segmental masonry
            // arch; 0.09 m is half the rendered floor slab.
            let floor_elevation_metres = wall.gate_height_metres
                + spec.arch_ring_depth.metres()
                + spec.arch_rise.metres()
                + 0.09;
            let radius = spec.tower_diameter.metres() * 0.5;
            let tower_offset = spec.gate_width.metres() * 0.5 + spec.jamb_reveal.metres() + radius;
            let half_along = tower_offset - (radius - spec.chord_bearing.metres());
            let chamber_size = if tangent.x.abs() >= tangent.y.abs() {
                Vec2::new(half_along * 2.0, spec.chamber_depth.metres())
            } else {
                Vec2::new(spec.chamber_depth.metres(), half_along * 2.0)
            };
            let chamber_centre = threshold;
            let from_walk_index = wall_walks
                .iter()
                .position(|walk| {
                    matches!(
                        walk,
                        WallWalk::Linear { start, end, .. }
                            if (*start - wall.start).length_squared() < 0.001
                                && (*end - wall.end).length_squared() < 0.001
                    )
                })
                .unwrap_or(0);
            let landing_size = if tangent.x.abs() >= tangent.y.abs() {
                Vec2::new(1.0, 1.4)
            } else {
                Vec2::new(1.4, 1.0)
            };
            let landing_depth_offset = spec.chamber_depth.metres() * 0.5 + 0.6;
            let top_landing_centre = threshold - tangent * 1.9 + inward * landing_depth_offset;
            let bottom_landing_centre = threshold + tangent * 1.9 + inward * landing_depth_offset;
            let flight_top = top_landing_centre + tangent * 0.5;
            let flight_bottom = bottom_landing_centre - tangent * 0.5;
            let door_position =
                threshold + tangent * 1.9 + inward * (spec.chamber_depth.metres() * 0.5);
            let mut access_supports = Vec::new();
            for (centre, top) in [
                (
                    top_landing_centre - tangent * 0.38 + inward * 0.42,
                    wall.height_metres,
                ),
                (
                    top_landing_centre + tangent * 0.38 + inward * 0.42,
                    wall.height_metres,
                ),
                (
                    bottom_landing_centre - tangent * 0.38 + inward * 0.42,
                    floor_elevation_metres,
                ),
                (
                    bottom_landing_centre + tangent * 0.38 + inward * 0.42,
                    floor_elevation_metres,
                ),
                (
                    flight_top.lerp(flight_bottom, 0.33) + inward * 0.42,
                    wall.height_metres + (floor_elevation_metres - wall.height_metres) * 0.33,
                ),
                (
                    flight_top.lerp(flight_bottom, 0.67) + inward * 0.42,
                    wall.height_metres + (floor_elevation_metres - wall.height_metres) * 0.67,
                ),
            ] {
                access_supports.push(GuardChamberSupport {
                    centre,
                    size: Vec2::splat(0.28),
                    base_elevation_metres: 0.0,
                    top_elevation_metres: top,
                });
            }
            let landing_along = landing_size.dot(tangent.abs()) * 0.5;
            let landing_depth = landing_size.dot(inward.abs()) * 0.5;
            let guard = |start, end, elevation_metres| AccessGuardSegment {
                start,
                end,
                elevation_metres,
                height_metres: 1.0,
            };
            let landing_guards = vec![
                guard(
                    top_landing_centre - tangent * landing_along + inward * landing_depth,
                    top_landing_centre + tangent * landing_along + inward * landing_depth,
                    wall.height_metres,
                ),
                guard(
                    top_landing_centre - tangent * landing_along - inward * landing_depth,
                    top_landing_centre - tangent * landing_along + inward * landing_depth,
                    wall.height_metres,
                ),
                guard(
                    bottom_landing_centre - tangent * landing_along + inward * landing_depth,
                    bottom_landing_centre + tangent * landing_along + inward * landing_depth,
                    floor_elevation_metres,
                ),
                guard(
                    bottom_landing_centre + tangent * landing_along - inward * landing_depth,
                    bottom_landing_centre + tangent * landing_along + inward * landing_depth,
                    floor_elevation_metres,
                ),
            ];
            let wall_ledger = AccessLedger {
                centre: threshold + inward * (spec.chamber_depth.metres() * 0.5 + 0.08),
                size: tangent.abs() * 4.8 + inward.abs() * 0.22,
                elevation_metres: floor_elevation_metres + 0.28,
                height_metres: 0.32,
            };
            let lateral_braces = vec![
                AccessBrace {
                    start: top_landing_centre - tangent * 0.38 + inward * 0.42,
                    start_elevation_metres: wall.height_metres - 0.2,
                    end: top_landing_centre - tangent * 0.38 - inward * 0.55,
                    end_elevation_metres: floor_elevation_metres + 0.5,
                    thickness_metres: 0.18,
                },
                AccessBrace {
                    start: top_landing_centre + tangent * 0.38 + inward * 0.42,
                    start_elevation_metres: wall.height_metres - 0.2,
                    end: top_landing_centre + tangent * 0.38 - inward * 0.55,
                    end_elevation_metres: floor_elevation_metres + 0.5,
                    thickness_metres: 0.18,
                },
                AccessBrace {
                    start: bottom_landing_centre - tangent * 0.38 + inward * 0.42,
                    start_elevation_metres: floor_elevation_metres - 0.2,
                    end: bottom_landing_centre - tangent * 0.38 - inward * 0.55,
                    end_elevation_metres: wall_ledger.elevation_metres,
                    thickness_metres: 0.18,
                },
                AccessBrace {
                    start: bottom_landing_centre + tangent * 0.38 + inward * 0.42,
                    start_elevation_metres: floor_elevation_metres - 0.2,
                    end: bottom_landing_centre + tangent * 0.38 - inward * 0.55,
                    end_elevation_metres: wall_ledger.elevation_metres,
                    thickness_metres: 0.18,
                },
                AccessBrace {
                    start: flight_top + inward * 0.42,
                    start_elevation_metres: wall.height_metres - 0.35,
                    end: flight_bottom + inward * 0.42,
                    end_elevation_metres: floor_elevation_metres - 0.75,
                    thickness_metres: 0.16,
                },
                AccessBrace {
                    start: flight_bottom + inward * 0.42,
                    start_elevation_metres: floor_elevation_metres - 0.35,
                    end: flight_top + inward * 0.42,
                    end_elevation_metres: wall.height_metres - 1.25,
                    thickness_metres: 0.16,
                },
            ];
            let guard_chamber = GateGuardChamber {
                centre: chamber_centre,
                size: chamber_size,
                floor_elevation_metres,
                clear_height_metres: 2.1,
                supporting_wall_index: wall_index,
                supports: Vec::new(),
                access: GuardChamberAccess {
                    from_walk_index,
                    envelope: TraversalEnvelope {
                        width_metres: 1.0,
                        height_metres: 1.9,
                    },
                    top_landing: AccessLanding {
                        centre: top_landing_centre,
                        size: landing_size,
                        elevation_metres: wall.height_metres,
                    },
                    flight: AccessStairFlight {
                        top: flight_top,
                        bottom: flight_bottom,
                        top_elevation_metres: wall.height_metres,
                        bottom_elevation_metres: floor_elevation_metres,
                        riser_count: 10,
                        going_metres: 0.28,
                        nosing_metres: 0.03,
                    },
                    bottom_landing: AccessLanding {
                        centre: bottom_landing_centre,
                        size: landing_size,
                        elevation_metres: floor_elevation_metres,
                    },
                    top_walk_opening: AccessDoor {
                        position: threshold - tangent * 1.9
                            + inward * (spec.chamber_depth.metres() * 0.5),
                        facing: wall.outward.opposite(),
                        threshold_elevation_metres: wall.height_metres,
                        width_metres: 1.0,
                        clear_height_metres: 1.9,
                        swing_inward: false,
                    },
                    door: AccessDoor {
                        position: door_position,
                        facing: wall.outward.opposite(),
                        threshold_elevation_metres: floor_elevation_metres,
                        width_metres: 1.0,
                        clear_height_metres: 2.0,
                        swing_inward: true,
                    },
                    roof_clearance_opening: AccessLanding {
                        centre: threshold - tangent * 1.9
                            + inward * (spec.chamber_depth.metres() * 0.5),
                        size: if tangent.x.abs() >= tangent.y.abs() {
                            Vec2::new(1.0, spec.chamber_depth.metres())
                        } else {
                            Vec2::new(spec.chamber_depth.metres(), 1.0)
                        },
                        elevation_metres: floor_elevation_metres + 2.1,
                    },
                    support_posts: access_supports,
                    landing_guards,
                    flight_guard_height_metres: 1.0,
                    wall_ledger,
                    lateral_braces,
                },
                openings: vec![
                    GuardChamberOpening {
                        kind: GuardOpeningKind::OutwardObservation,
                        position: threshold + outward * (spec.chamber_depth.metres() * 0.5),
                        sill_elevation_metres: floor_elevation_metres + 0.85,
                        width_metres: 0.35,
                        clear_height_metres: 0.8,
                        facing: wall.outward,
                        target: approach,
                    },
                    GuardChamberOpening {
                        kind: GuardOpeningKind::DownwardDefense,
                        position: threshold + inward * 0.18,
                        sill_elevation_metres: floor_elevation_metres,
                        width_metres: 0.45,
                        clear_height_metres: 0.45,
                        facing: wall.outward,
                        target: threshold,
                    },
                ],
                operating_positions: vec![GateOperatingPosition {
                    closure_index: 1,
                    position: threshold + inward * 0.55,
                    elevation_metres: floor_elevation_metres,
                }],
                load_path: GatehouseLoadPath::BondedTowerBearing {
                    left_tower_index: tower_indices[0],
                    right_tower_index: tower_indices[1],
                    bearing_depth: spec.chord_bearing,
                    arch_centre: threshold,
                    arch_spring_elevation_metres: wall.gate_height_metres,
                    arch_ring_depth: spec.arch_ring_depth,
                    arch_rise: spec.arch_rise,
                    curtain_return_bond: spec.curtain_return_bond,
                },
            };
            Some(GateDefense {
                curtain_wall_index: wall_index,
                threshold,
                approach,
                passage_profile: crate::GatePassageProfile {
                    width_metres: spec.gate_width.metres(),
                    spring_height_metres: wall.gate_height_metres,
                    arch_rise_metres: spec.arch_rise.metres(),
                },
                firing_positions,
                closures: vec![
                    GateClosure {
                        curtain_wall_index: wall_index,
                        kind: GateClosureKind::HeavyLeaves,
                        inward_offset_metres: 0.08,
                        coverage: crate::GatePassageProfile {
                            width_metres: spec.gate_width.metres(),
                            spring_height_metres: wall.gate_height_metres,
                            arch_rise_metres: spec.arch_rise.metres(),
                        },
                    },
                    GateClosure {
                        curtain_wall_index: wall_index,
                        kind: GateClosureKind::Portcullis,
                        inward_offset_metres: 0.55,
                        coverage: crate::GatePassageProfile {
                            width_metres: spec.gate_width.metres(),
                            spring_height_metres: wall.gate_height_metres,
                            arch_rise_metres: spec.arch_rise.metres(),
                        },
                    },
                ],
                guard_chamber,
            })
        })
        .collect()
}
