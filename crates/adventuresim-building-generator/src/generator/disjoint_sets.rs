struct DisjointSets {
    parents: Vec<usize>,
    components: usize,
}

impl DisjointSets {
    fn new(len: usize) -> Self {
        Self {
            parents: (0..len).collect(),
            components: len,
        }
    }

    fn find(&mut self, mut value: usize) -> usize {
        while self.parents[value] != value {
            self.parents[value] = self.parents[self.parents[value]];
            value = self.parents[value];
        }
        value
    }

    fn union(&mut self, left: usize, right: usize) -> bool {
        let left = self.find(left);
        let right = self.find(right);
        if left == right {
            return false;
        }
        self.parents[right] = left;
        self.components -= 1;
        true
    }

    const fn component_count(&self) -> usize {
        self.components
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BuildingArchetype, BuildingProgram, DormerKind, OpeningKind, RoofKind, TimberFrameStyle,
    };

    #[test]
    fn fixture_seed_matrix_generates_audit_clean_buildings() {
        // Exercise seeds selected to cover zero, adjacent values, the curated
        // proof seeds, large values, and wrapping arithmetic boundaries.
        const SEEDS: [u64; 8] = [0, 1, 2, 17, 42, 47, 101, u64::MAX];

        for archetype in BuildingArchetype::ALL {
            for seed in SEEDS {
                let program = BuildingProgram::fixture(archetype, seed);
                let plan = generate(&program).unwrap_or_else(|error| {
                    panic!("{archetype:?} seed {seed} must be supported: {error:?}")
                });
                assert!(
                    crate::audit_plan(&plan).is_empty(),
                    "{archetype:?} seed {seed} escaped the public boundary with audit issues"
                );
            }
        }
    }

    #[test]
    fn invalid_generated_plan_is_rejected_at_the_public_boundary() {
        let mut plan = generate_unchecked(
            &BuildingProgram::fixture(BuildingArchetype::TownHouse, 42),
            &[],
        )
        .unwrap();
        let removed = plan.resolved_geometry.solids.pop().unwrap();

        let error = validate_generated_plan(plan).unwrap_err();
        let GenerationError::StructuralContract {
            issues_count,
            issues,
        } = error
        else {
            panic!("invalid resolved plan must fail the structural contract");
        };
        assert_eq!(issues_count, issues.len());
        assert!(!issues.is_empty(), "removing {removed:?} must be audited");
    }

    #[test]
    fn malformed_high_level_program_returns_a_typed_error() {
        let mut program = BuildingProgram::fixture(BuildingArchetype::TownHouse, 42);
        program.storeys[0].rooms.clear();
        assert!(matches!(
            generate(&program),
            Err(GenerationError::EmptyStorey { level: 0 })
        ));
    }

    #[test]
    fn multi_storey_program_without_vertical_connection_is_rejected() {
        let mut program = BuildingProgram::fixture(BuildingArchetype::TownHouse, 42);
        program.vertical_connections.clear();
        assert!(matches!(
            generate(&program),
            Err(GenerationError::UnsatisfiedVerticalCirculation { .. })
        ));

        let mut missing_landing_room = BuildingProgram::fixture(BuildingArchetype::TownHouse, 42);
        missing_landing_room.storeys[1]
            .rooms
            .retain(|room| room.kind != RoomKind::StairHall);
        assert!(matches!(
            generate(&missing_landing_room),
            Err(GenerationError::UnsatisfiedVerticalCirculation { .. })
        ));

        let mut uncovered_storey =
            BuildingProgram::fixture(BuildingArchetype::FachwerkMerchantHouse, 42);
        uncovered_storey.vertical_connections[0] = VerticalConnectionRequirement::StraightStair {
            lowest_storey: 0,
            highest_storey: 1,
            landing_room: RoomKind::StairHall,
        };
        assert!(matches!(
            generate(&uncovered_storey),
            Err(GenerationError::UnsatisfiedVerticalCirculation { .. })
        ));
    }

    #[test]
    fn civilian_programmes_reserve_each_straight_stair_arrival_in_a_connected_hall() {
        for archetype in [
            BuildingArchetype::TownHouse,
            BuildingArchetype::FachwerkMerchantHouse,
            BuildingArchetype::RenaissanceTownHall,
        ] {
            let plan = generate(&BuildingProgram::fixture(archetype, 42))
                .unwrap_or_else(|error| panic!("{archetype:?}: {error:?}"));
            let straight_stairs = plan
                .stairs
                .iter()
                .filter(|stair| matches!(stair, Stair::Straight { .. }))
                .count();
            assert_eq!(straight_stairs, plan.storeys.len() - 1, "{archetype:?}");
            assert!(
                crate::audit_plan(&plan)
                    .iter()
                    .all(|issue| issue.code != "invalid_vertical_circulation"),
                "{archetype:?}"
            );
        }
    }

    #[test]
    fn circulation_audit_rejects_a_stair_moved_out_of_its_reserved_hall() {
        let mut plan =
            generate(&BuildingProgram::fixture(BuildingArchetype::TownHouse, 42)).unwrap();
        let Stair::Straight { start, .. } = &mut plan.stairs[0] else {
            panic!("town house has a straight stair");
        };
        *start = Vec2::splat(-20.0);
        assert!(
            crate::audit_plan(&plan)
                .iter()
                .any(|issue| issue.code == "invalid_vertical_circulation")
        );
    }

    #[test]
    fn town_house_seed_one_has_a_clear_timber_entry_to_stair_route() {
        let plan = generate(&BuildingProgram::fixture(BuildingArchetype::TownHouse, 1))
            .expect("town-house seed one has a traversable timber route");
        assert!(
            crate::audit_plan(&plan)
                .iter()
                .all(|issue| issue.code != "invalid_timber_circulation")
        );
    }

    #[test]
    fn editor_window_command_is_transactional_and_serializable() {
        let document = BuildingDocument::fixture(BuildingArchetype::TownHouse, 42);
        let base = generate_document(&document).unwrap();
        let storey = &base.storeys[1];
        let (wall_index, wall) = storey
            .walls
            .iter()
            .enumerate()
            .find(|(index, wall)| {
                wall.exterior() && !storey.openings.iter().any(|opening| opening.wall == *index)
            })
            .expect("fixture has an unopened exterior wall");
        let selector = crate::WallSelector {
            storey_level: storey.level,
            cell: wall.cell,
            direction: wall.direction,
        };
        let (edited, plan) = edit_document(
            &document,
            BuildingEdit::AddOpening {
                wall: selector,
                opening_kind: OpeningKind::Window,
                width_metres: 0.80,
                sill_metres: 0.90,
                height_metres: 1.10,
            },
        )
        .unwrap();
        assert!(crate::audit_plan(&plan).is_empty());
        assert!(
            plan.storeys[1].openings.iter().any(|opening| {
                opening.wall == wall_index && opening.kind == OpeningKind::Window
            })
        );

        let encoded = serde_json::to_vec(&edited).unwrap();
        let decoded: BuildingDocument = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(
            serde_json::to_vec(&generate_document(&decoded).unwrap()).unwrap(),
            serde_json::to_vec(&plan).unwrap()
        );
    }

    #[test]
    fn editor_opening_command_supports_audited_doors() {
        let document = BuildingDocument::fixture(BuildingArchetype::TownHouse, 42);
        let plan = generate_document(&document).unwrap();
        let storey = &plan.storeys[1];
        let wall = storey
            .walls
            .iter()
            .enumerate()
            .find(|(index, wall)| {
                wall.exterior() && !storey.openings.iter().any(|opening| opening.wall == *index)
            })
            .map(|(_, wall)| wall)
            .expect("fixture has an unopened exterior wall");
        let (edited, edited_plan) = edit_document(
            &document,
            BuildingEdit::AddOpening {
                wall: crate::WallSelector {
                    storey_level: storey.level,
                    cell: wall.cell,
                    direction: wall.direction,
                },
                opening_kind: OpeningKind::Door,
                width_metres: 0.95,
                sill_metres: 0.0,
                height_metres: 2.1,
            },
        )
        .unwrap();
        assert!(crate::audit_plan(&edited_plan).is_empty());
        assert!(edited.edits.iter().any(|edit| matches!(
            edit,
            BuildingEdit::AddOpening {
                opening_kind: OpeningKind::Door,
                ..
            }
        )));
    }

    #[test]
    fn invalid_editor_command_preserves_the_previous_document() {
        let document = BuildingDocument::fixture(BuildingArchetype::TownHouse, 42);
        let plan = generate_document(&document).unwrap();
        let opening = plan.storeys[0].openings[0];
        let wall = plan.storeys[0].walls[opening.wall];
        let result = edit_document(
            &document,
            BuildingEdit::AddOpening {
                wall: crate::WallSelector {
                    storey_level: 0,
                    cell: wall.cell,
                    direction: wall.direction,
                },
                opening_kind: OpeningKind::Window,
                width_metres: 0.8,
                sill_metres: 0.9,
                height_metres: 1.1,
            },
        );
        assert!(matches!(result, Err(GenerationError::EditConflict(_))));
        assert!(document.edits.is_empty());
    }

    #[test]
    fn editor_document_rejects_unknown_schema_and_inapplicable_styles() {
        let mut future = BuildingDocument::fixture(BuildingArchetype::TownHouse, 42);
        future.schema_version += 1;
        assert!(matches!(
            generate_document(&future),
            Err(GenerationError::UnsupportedDocumentSchema { .. })
        ));

        let cathedral = BuildingDocument::fixture(BuildingArchetype::Cathedral, 42);
        assert!(matches!(
            edit_document(
                &cathedral,
                BuildingEdit::SetWallStyle {
                    style: crate::WallStyle::TimberFrame,
                }
            ),
            Err(GenerationError::UnsupportedEdit(_))
        ));
        assert!(matches!(
            edit_document(
                &cathedral,
                BuildingEdit::SetTimberFrameStyle {
                    style: crate::TimberFrameStyle::EarlyModernOrnate,
                }
            ),
            Err(GenerationError::UnsupportedEdit(_))
        ));
    }

    #[test]
    fn editor_style_edits_regenerate_a_valid_civilian_building() {
        let document = BuildingDocument::fixture(BuildingArchetype::FachwerkMerchantHouse, 42);
        let (document, plan) = edit_document(
            &document,
            BuildingEdit::SetWallStyle {
                style: crate::WallStyle::Brick,
            },
        )
        .unwrap();
        assert_eq!(plan.wall_style, crate::WallStyle::Brick);
        assert!(crate::audit_plan(&plan).is_empty());
        let original_braces = plan
            .timber_frame
            .as_ref()
            .unwrap()
            .members
            .iter()
            .filter(|member| member.role == crate::TimberMemberRole::StoreyBrace)
            .map(|member| (member.start, member.end))
            .collect::<Vec<_>>();

        let (_, plan) = edit_document(
            &document,
            BuildingEdit::SetTimberFrameStyle {
                style: crate::TimberFrameStyle::NorthernCloseStudded,
            },
        )
        .unwrap();
        assert_eq!(
            plan.timber_frame_style,
            Some(crate::TimberFrameStyle::NorthernCloseStudded)
        );
        let edited_braces = plan
            .timber_frame
            .as_ref()
            .unwrap()
            .members
            .iter()
            .filter(|member| member.role == crate::TimberMemberRole::StoreyBrace)
            .map(|member| (member.start, member.end))
            .collect::<Vec<_>>();
        assert_ne!(original_braces, edited_braces);
        assert!(crate::audit_plan(&plan).is_empty());
    }

    #[test]
    fn roof_pitch_handle_recomputes_graph_or_rejects_topology_events() {
        let mut plain = generate(&BuildingProgram::fixture(
            BuildingArchetype::CastleGatehouse,
            42,
        ))
        .unwrap();
        let id = plain
            .roof_assemblies
            .iter()
            .find(|roof| roof.children.is_empty() && roof.parent.is_none())
            .unwrap()
            .id;
        let initial_enclosure_apex = plain
            .roof_assemblies
            .iter()
            .find(|roof| roof.id == id)
            .and_then(|roof| roof.enclosure_faces.first())
            .and_then(|face| {
                face.polygon
                    .iter()
                    .map(|point| point.y)
                    .max_by(f32::total_cmp)
            });
        for half_degree in 30..=150 {
            let pitch = half_degree as f32 * 0.5;
            set_roof_pitch(&mut plain, id, pitch).unwrap();
            let roof = plain
                .roof_assemblies
                .iter()
                .find(|roof| roof.id == id)
                .unwrap();
            assert!(
                roof.faces
                    .iter()
                    .all(|face| (face.pitch_degrees - pitch).abs() < 0.001)
            );
            if roof.kind == RoofKind::Gable {
                let roof_apex = roof
                    .faces
                    .iter()
                    .flat_map(|face| &face.polygon)
                    .map(|point| point.y)
                    .fold(f32::NEG_INFINITY, f32::max);
                let enclosure_apex = roof
                    .enclosure_faces
                    .iter()
                    .flat_map(|face| &face.polygon)
                    .map(|point| point.y)
                    .fold(f32::NEG_INFINITY, f32::max);
                assert!((roof_apex - enclosure_apex).abs() <= 0.01);
            }
        }
        let final_enclosure_apex = plain
            .roof_assemblies
            .iter()
            .find(|roof| roof.id == id)
            .and_then(|roof| roof.enclosure_faces.first())
            .and_then(|face| {
                face.polygon
                    .iter()
                    .map(|point| point.y)
                    .max_by(f32::total_cmp)
            });
        assert_ne!(initial_enclosure_apex, final_enclosure_apex);
        assert_eq!(
            set_roof_pitch(&mut plain, id, 14.5),
            Err(RoofEditError::PitchOutsideProjectRange)
        );
        let mut merchant = generate(&BuildingProgram::fixture(
            BuildingArchetype::FachwerkMerchantHouse,
            42,
        ))
        .unwrap();
        let parent = merchant.roof_assemblies[0].id;
        assert_eq!(
            set_roof_pitch(&mut merchant, parent, 60.0),
            Err(RoofEditError::TopologyEvent)
        );
    }

    #[test]
    fn courtyard_roof_graph_owns_four_drained_peer_valleys() {
        let plan = generate(&BuildingProgram::fixture(
            BuildingArchetype::CourtyardCastle,
            42,
        ))
        .unwrap();
        let valleys = plan
            .roof_assemblies
            .iter()
            .flat_map(|roof| &roof.edges)
            .filter(|edge| edge.kind == RoofEdgeKind::Valley)
            .collect::<Vec<_>>();
        assert_eq!(valleys.len(), 4);
        assert!(valleys.iter().all(|edge| {
            edge.adjacent_faces.len() == 2
                && edge.flashing.is_some()
                && edge.drainage_terminal.is_some()
        }));
    }

    #[test]
    fn every_fixture_is_deterministic_connected_and_room_complete() {
        for archetype in BuildingArchetype::ALL {
            let program = BuildingProgram::fixture(archetype, 42);
            let first = generate(&program).unwrap();
            let second = generate(&program).unwrap();
            let first_json = serde_json::to_vec(&first).unwrap();
            let second_json = serde_json::to_vec(&second).unwrap();
            if first_json != second_json {
                let offset = first_json
                    .iter()
                    .zip(&second_json)
                    .position(|(left, right)| left != right)
                    .unwrap_or(first_json.len().min(second_json.len()));
                let start = offset.saturating_sub(80);
                let left_end = (offset + 160).min(first_json.len());
                let right_end = (offset + 160).min(second_json.len());
                panic!(
                    "{archetype:?} must be reproducible at byte {offset}: left={} right={}",
                    String::from_utf8_lossy(&first_json[start..left_end]),
                    String::from_utf8_lossy(&second_json[start..right_end]),
                );
            }
            for storey in &first.storeys {
                assert!(storey.rooms.iter().all(|room| !room.cells.is_empty()));
                assert!(
                    storey
                        .rooms
                        .iter()
                        .all(|room| cells_are_connected(&room.cells))
                );
                assert_eq!(
                    storey
                        .rooms
                        .iter()
                        .flat_map(|room| room.cells.iter())
                        .collect::<HashSet<_>>()
                        .len(),
                    storey
                        .rooms
                        .iter()
                        .map(|room| room.cells.len())
                        .sum::<usize>()
                );
                assert!(
                    storey
                        .openings
                        .iter()
                        .all(|opening| opening.wall < storey.walls.len())
                );
                if first.church.is_some() {
                    assert!(
                        storey.openings.is_empty(),
                        "church rejects legacy opening overlays"
                    );
                    assert!(
                        first
                            .opening_assemblies
                            .iter()
                            .filter(|opening| opening.use_kind == crate::OpeningUse::Door)
                            .count()
                            >= 2
                    );
                } else {
                    assert!(
                        storey
                            .openings
                            .iter()
                            .any(|opening| opening.kind == OpeningKind::Door)
                    );
                }
            }
        }
    }

    #[test]
    fn civilian_profiles_have_steep_independent_roof_pieces() {
        let town = generate(&BuildingProgram::fixture(BuildingArchetype::TownHouse, 7)).unwrap();
        assert_eq!(town.roofs.len(), 1);
        assert_eq!(town.roofs[0].kind, RoofKind::Gable);
        assert!(town.roofs[0].pitch_degrees >= 50.0);

        let hall = generate(&BuildingProgram::fixture(BuildingArchetype::HallHouse, 7)).unwrap();
        assert_eq!(hall.roofs[0].kind, RoofKind::HalfHip);
        assert!(hall.roofs[0].eave_metres >= 0.5);
    }

    #[test]
    fn ornate_fachwerk_fixtures_have_projecting_storeys_and_complex_roofscapes() {
        for archetype in [
            BuildingArchetype::FachwerkMerchantHouse,
            BuildingArchetype::RenaissanceTownHall,
        ] {
            let plan = generate(&BuildingProgram::fixture(archetype, 17)).unwrap();
            assert_eq!(
                plan.timber_frame_style,
                Some(TimberFrameStyle::EarlyModernOrnate)
            );
            assert!(plan.upper_storey_projection_metres >= 0.2);
            // Complexity is expressed by authoritative child roof assemblies,
            // not an independent intersecting RoofPiece floating above the
            // parent weather face.
            assert_eq!(plan.roofs.len(), 1);
            let expected_dormers = if archetype == BuildingArchetype::FachwerkMerchantHouse {
                3
            } else {
                2
            };
            assert!(plan.roof_dormers.len() >= expected_dormers);
            assert!(plan.roof_assemblies.len() > expected_dormers);
            if archetype == BuildingArchetype::FachwerkMerchantHouse {
                assert!(
                    plan.roof_dormers
                        .iter()
                        .any(|dormer| dormer.kind == DormerKind::TransverseGable)
                );
            }
        }
        let civic = generate(&BuildingProgram::fixture(
            BuildingArchetype::RenaissanceTownHall,
            17,
        ))
        .unwrap();
        assert!(
            civic
                .roofs
                .iter()
                .any(|roof| roof.gable_profile != GableProfile::Plain)
        );
    }

    #[test]
    fn castle_profiles_include_round_towers_spiral_stairs_and_defensive_crowns() {
        for archetype in [
            BuildingArchetype::CastleGatehouse,
            BuildingArchetype::CourtyardCastle,
            BuildingArchetype::WalledKeep,
        ] {
            let plan = generate(&BuildingProgram::fixture(archetype, 19)).unwrap();
            assert!(plan.towers.len() >= 2);
            assert!(
                plan.stairs
                    .iter()
                    .any(|stair| matches!(stair, Stair::Spiral { .. }))
            );
            assert!(!plan.battlements.is_empty());
        }
    }

    #[test]
    fn castle_battlements_have_continuous_wall_walks_and_tower_access() {
        for archetype in [
            BuildingArchetype::CastleGatehouse,
            BuildingArchetype::CourtyardCastle,
            BuildingArchetype::WalledKeep,
        ] {
            let plan = generate(&BuildingProgram::fixture(archetype, 29)).unwrap();
            let expected_linear_walks = plan
                .battlements
                .iter()
                .filter(|run| run.kind != BattlementKind::Breteche)
                .count();
            assert_eq!(
                plan.wall_walks
                    .iter()
                    .filter(|walk| matches!(walk, WallWalk::Linear { .. }))
                    .count(),
                expected_linear_walks
            );
            assert_eq!(
                plan.wall_walks
                    .iter()
                    .filter(|walk| matches!(walk, WallWalk::Round { .. }))
                    .count(),
                plan.towers.len()
            );
            for tower in &plan.towers {
                assert!(plan.stairs.iter().any(|stair| {
                    matches!(
                        stair,
                        Stair::Spiral {
                            centre,
                            base_height_metres,
                            rise_metres,
                            ..
                        } if *centre == tower.centre_metres()
                            && (*base_height_metres + *rise_metres
                                - tower.wall_height_metres)
                                .abs()
                                < 0.001
                    )
                }));
            }
            assert!(plan.towers.iter().enumerate().all(|(tower_index, _)| {
                plan.tower_portals.iter().any(|portal| {
                    portal.tower_index == tower_index
                        && portal.kind == TowerPortalKind::GroundStairEntrance
                })
            }));
            assert!(plan.defensive_junctions.iter().all(|junction| {
                let pair = [junction.walk_a, junction.walk_b];
                let has_round = pair
                    .iter()
                    .any(|&index| matches!(plan.wall_walks[index], WallWalk::Round { .. }));
                let linear = pair
                    .iter()
                    .find(|&&index| matches!(plan.wall_walks[index], WallWalk::Linear { .. }));
                !has_round
                    || linear.is_some_and(|&walk_index| {
                        plan.tower_portals.iter().any(|portal| {
                            portal.kind == TowerPortalKind::WallWalkJunction { walk_index }
                        })
                    })
            }));
            let wall_top = plan.storeys.len() as f32 * plan.storey_height_metres;
            for run in plan
                .battlements
                .iter()
                .filter(|run| run.kind != BattlementKind::Breteche)
            {
                assert!(
                    (run.base_height_metres - wall_top).abs() < 0.001
                        || plan.curtain_walls.iter().any(|wall| {
                            (wall.height_metres - run.base_height_metres).abs() < 0.001
                                && ((wall.start - run.start).length_squared() < 0.001)
                                && ((wall.end - run.end).length_squared() < 0.001)
                        })
                );
            }
        }
    }

    #[test]
    fn fortified_exteriors_use_narrow_firing_loops_instead_of_glazed_windows() {
        for archetype in [
            BuildingArchetype::CastleGatehouse,
            BuildingArchetype::CourtyardCastle,
            BuildingArchetype::WalledKeep,
        ] {
            let plan = generate(&BuildingProgram::fixture(archetype, 31)).unwrap();
            let exterior_openings = plan.storeys.iter().flat_map(|storey| {
                storey
                    .openings
                    .iter()
                    .filter(|opening| storey.walls[opening.wall].exterior())
            });
            let mut firing_loops = 0;
            for opening in exterior_openings {
                assert_ne!(opening.kind, OpeningKind::Window);
                if opening.kind == OpeningKind::ArrowSlit {
                    firing_loops += 1;
                    assert!(opening.width_metres <= 0.2);
                    assert!(opening.height_metres >= 0.8);
                }
            }
            assert!(firing_loops > 0);
        }
    }

    #[test]
    fn castle_fixtures_separate_accepted_masonry_crowns_from_legacy_vocabulary() {
        let plans = [
            generate(&BuildingProgram::fixture(
                BuildingArchetype::CastleGatehouse,
                23,
            ))
            .unwrap(),
            generate(&BuildingProgram::fixture(
                BuildingArchetype::CastleGatehouse,
                201,
            ))
            .unwrap(),
            generate(&BuildingProgram::fixture(
                BuildingArchetype::CastleGatehouse,
                202,
            ))
            .unwrap(),
            generate(&BuildingProgram::fixture(
                BuildingArchetype::CastleGatehouse,
                203,
            ))
            .unwrap(),
            generate(&BuildingProgram::fixture(
                BuildingArchetype::CourtyardCastle,
                23,
            ))
            .unwrap(),
            generate(&BuildingProgram::fixture(BuildingArchetype::WalledKeep, 23)).unwrap(),
        ];
        let kinds = plans
            .iter()
            .flat_map(|plan| {
                plan.battlements
                    .iter()
                    .map(|run| run.kind)
                    .chain(plan.towers.iter().filter_map(|tower| tower.battlement))
            })
            .collect::<HashSet<_>>();
        for expected in [
            BattlementKind::Crenellated,
            BattlementKind::Machicolated,
            BattlementKind::OpenHoarding,
            BattlementKind::RoofedHoarding,
            BattlementKind::CoveredWallWalk,
            BattlementKind::Breteche,
        ] {
            assert!(kinds.contains(&expected), "missing {expected:?}");
        }
        assert!(plans.iter().flat_map(|plan| &plan.crowns).all(|crown| {
            crown.pattern == CrownPattern::Crenellated && crown.material == CrownMaterial::Masonry
        }));
        assert!(plans.iter().any(|plan| !plan.bartizans.is_empty()));
        for plan in &plans[..4] {
            let deployed_kinds = plan
                .projected_defenses
                .iter()
                .filter(|defense| defense.deployment != ProjectedDefenseDeployment::SocketsOnly)
                .map(|defense| defense.kind)
                .collect::<HashSet<_>>();
            assert!(
                deployed_kinds.len() <= 1,
                "one coherent castle state must not become a projected-defense catalogue: {deployed_kinds:?}"
            );
        }
    }

    #[test]
    fn courtyard_footprint_leaves_a_real_open_court() {
        let program = BuildingProgram::fixture(BuildingArchetype::CourtyardCastle, 3);
        let plan = generate(&program).unwrap();
        let Footprint::Courtyard {
            width, depth, wing, ..
        } = plan.footprint
        else {
            panic!("expected courtyard")
        };
        let centre = Cell::new((width / 2) as i16, (depth / 2) as i16);
        assert!(centre.x >= wing as i16 && centre.x < (width - wing) as i16);
        assert!(centre.z >= wing as i16 && centre.z < (depth - wing) as i16);
        assert!(
            plan.storeys[0]
                .rooms
                .iter()
                .all(|room| !room.cells.contains(&centre))
        );
        let passage = plan.storeys[0]
            .rooms
            .iter()
            .find(|room| room.kind == RoomKind::Passage)
            .unwrap();
        assert_eq!(passage.cells.len(), usize::from(wing * 4));
        assert_eq!(
            plan.storeys[0]
                .openings
                .iter()
                .filter(|opening| opening.kind == OpeningKind::Gate)
                .count(),
            4
        );
    }

    #[test]
    fn courtyard_castle_uses_unroofed_towers_and_permanent_stone_crowns() {
        let plan = generate(&BuildingProgram::fixture(
            BuildingArchetype::CourtyardCastle,
            37,
        ))
        .unwrap();
        assert!(plan.towers.iter().all(|tower| tower.roof.is_none()));
        assert!(plan.battlements.iter().all(|run| !matches!(
            run.kind,
            BattlementKind::OpenHoarding
                | BattlementKind::RoofedHoarding
                | BattlementKind::CoveredWallWalk
        )));
        let west = plan
            .battlements
            .iter()
            .find(|run| run.outward == Direction::West)
            .unwrap();
        assert_eq!(west.kind, BattlementKind::Crenellated);
        assert!(
            plan.towers
                .iter()
                .all(|tower| { tower.battlement == Some(BattlementKind::Crenellated) })
        );
        assert!(
            plan.battlements
                .iter()
                .filter(|run| run.kind != BattlementKind::Breteche)
                .all(|run| run.kind == BattlementKind::Crenellated)
        );
    }

    #[test]
    fn walled_keep_has_detached_outer_curtain_and_central_fighting_roof() {
        let plan = generate(&BuildingProgram::fixture(BuildingArchetype::WalledKeep, 41)).unwrap();
        assert_eq!(plan.curtain_walls.len(), 4);
        assert_eq!(plan.towers.len(), 6);
        assert_eq!(plan.defensive_circuits.len(), 2);
        assert!(
            plan.curtain_walls
                .iter()
                .any(|wall| wall.gate_width_metres.is_some())
        );
        let keep_top = plan.storeys.len() as f32 * plan.storey_height_metres;
        assert!(plan.battlements.iter().any(|run| {
            (run.base_height_metres - keep_top).abs() < 0.001
                && run.start.x >= 0.0
                && run.start.y >= 0.0
        }));
        assert!(plan.curtain_walls.iter().all(|wall| {
            wall.start.x < 0.0
                || wall.start.y < 0.0
                || wall.end.x > plan.dimensions_metres().x
                || wall.end.y > plan.dimensions_metres().y
        }));
        assert!(
            plan.curtain_walls
                .iter()
                .all(|wall| wall.thickness_metres >= 1.2)
        );
        assert!(
            plan.towers
                .iter()
                .all(|tower| tower.wall_thickness_metres >= 1.2)
        );
        let gate = plan
            .curtain_walls
            .iter()
            .find(|wall| wall.gate_width_metres.is_some())
            .unwrap();
        let gate_centre = (gate.start + gate.end) * 0.5;
        assert_eq!(
            plan.towers
                .iter()
                .filter(
                    |tower| (tower.centre_metres().y - gate_centre.y).abs() < 0.01
                        && (tower.centre_metres().x - gate_centre.x).abs() < 6.0
                )
                .count(),
            2
        );
        assert_eq!(plan.gate_defenses.len(), 1);
        assert_eq!(plan.gate_defenses[0].firing_positions.len(), 2);
        assert_eq!(plan.gate_defenses[0].closures.len(), 2);
        assert!(plan.gate_defenses[0].guard_chamber.size.element_product() >= 6.0);
        assert!(matches!(
            plan.gate_defenses[0].guard_chamber.load_path,
            GatehouseLoadPath::BondedTowerBearing { .. }
        ));
        assert!(!plan.gate_defenses[0].guard_chamber.openings.is_empty());
    }

    #[test]
    fn round_tower_diameter_and_anchor_are_discrete_grid_authority() {
        let plan = generate(&BuildingProgram::fixture(BuildingArchetype::WalledKeep, 61)).unwrap();
        let spec = plan.gatehouse_assemblies[0];
        assert_eq!(spec.tower_diameter.cells(), 4);
        assert_eq!(spec.tower_diameter.grid_units(), 120);
        assert_eq!(
            CellDiameter::try_from_grid_units(120),
            Some(spec.tower_diameter)
        );
        assert_eq!(CellDiameter::try_from_grid_units(119), None);
        assert_eq!(CellDiameter::new(0), None);
        let even = CellDiameter::new(4).unwrap();
        assert!(RoundTower::new(GridPoint::new(15, 0), even, 6.0, 1.2, None, None).is_none());
        assert!(serde_json::from_str::<CellDiameter>("0").is_err());
        assert!(serde_json::from_str::<GridLength>("-1").is_err());
        assert!(serde_json::from_str::<RoundTower>(r#"{"anchor":{"x":15,"z":0},"diameter":4,"wall_height_metres":6.0,"wall_thickness_metres":1.2,"roof":null,"battlement":null,"chord_interface":null}"#).is_err());
        for tower in &plan.towers {
            let metres = tower.anchor().metres();
            assert_eq!(metres, tower.centre_metres());
            assert_eq!(
                tower.diameter().grid_units() % crate::GRID_UNITS_PER_CELL,
                0
            );
        }
    }

    #[test]
    fn castle_round_shells_replace_intersecting_storey_wall_sources() {
        for archetype in [
            BuildingArchetype::CastleGatehouse,
            BuildingArchetype::CourtyardCastle,
        ] {
            let plan = generate(&BuildingProgram::fixture(archetype, 61)).unwrap();
            let round_walls = plan
                .wall_assemblies
                .iter()
                .filter(|wall| matches!(wall.source, crate::WallSourceId::RoundTower { .. }))
                .collect::<Vec<_>>();
            assert_eq!(round_walls.len(), plan.towers.len());
            let round_replacements = plan
                .wall_assemblies
                .iter()
                .filter(|wall| {
                    wall.replaced_by_owner.is_some_and(|replacement| {
                        round_walls.iter().any(|round| round.owner == replacement)
                    })
                })
                .collect::<Vec<_>>();
            assert!(!round_replacements.is_empty());
            for wall in round_replacements {
                let replacement = wall.replaced_by_owner.unwrap();
                assert!(wall.opening_ids.is_empty());
                assert!(wall.host_solids.iter().all(|id| {
                    plan.resolved_geometry
                        .solids
                        .iter()
                        .any(|solid| solid.id == *id && solid.owner == replacement)
                }));
            }
            if archetype == BuildingArchetype::CastleGatehouse {
                assert!(plan.towers.iter().all(|tower| {
                    tower.chord_interface.is_some() && tower.secondary_chord_interface.is_some()
                }));
                assert!(round_walls.iter().all(|wall| {
                    plan.resolved_geometry
                        .solids
                        .iter()
                        .find(|solid| solid.id == wall.host_solids[0])
                        .is_some_and(|solid| {
                            matches!(
                                solid.shape,
                                crate::ResolvedSolidShape::RoundTowerShell {
                                    chord_interfaces: [Some(_), Some(_)],
                                    ..
                                }
                            )
                        })
                }));
            }
        }
    }

    #[test]
    fn gatehouse_assembly_resolves_symmetrically_for_four_wall_orientations() {
        let spec = derive_gatehouse_assemblies(&BuildingProgram::fixture(
            BuildingArchetype::WalledKeep,
            62,
        ))[0];
        let walls = [
            CurtainWallRun {
                start: Vec2::new(-11.25, 0.0),
                end: Vec2::new(12.75, 0.0),
                height_metres: 6.0,
                thickness_metres: 1.2,
                outward: Direction::South,
                gate_width_metres: Some(3.2),
                gate_height_metres: 3.6,
            },
            CurtainWallRun {
                start: Vec2::new(12.75, 0.0),
                end: Vec2::new(-11.25, 0.0),
                height_metres: 6.0,
                thickness_metres: 1.2,
                outward: Direction::North,
                gate_width_metres: Some(3.2),
                gate_height_metres: 3.6,
            },
            CurtainWallRun {
                start: Vec2::new(0.0, -11.25),
                end: Vec2::new(0.0, 12.75),
                height_metres: 6.0,
                thickness_metres: 1.2,
                outward: Direction::East,
                gate_width_metres: Some(3.2),
                gate_height_metres: 3.6,
            },
            CurtainWallRun {
                start: Vec2::new(0.0, 12.75),
                end: Vec2::new(0.0, -11.25),
                height_metres: 6.0,
                thickness_metres: 1.2,
                outward: Direction::West,
                gate_width_metres: Some(3.2),
                gate_height_metres: 3.6,
            },
        ];
        let program = BuildingProgram::fixture(BuildingArchetype::WalledKeep, 62);
        for wall in walls {
            let towers = resolve_gatehouse_towers(spec, wall, 6.0).unwrap();
            let tangent = (wall.end - wall.start).normalize();
            let outward = direction_vector(wall.outward);
            let threshold = (wall.start + wall.end) * 0.5;
            let offsets = towers.map(|tower| tower.centre_metres() - threshold);
            assert!((offsets[0] + offsets[1]).length() < 0.001);
            assert!(
                offsets
                    .iter()
                    .all(|offset| offset.dot(direction_vector(wall.outward)).abs() < 0.001)
            );
            assert!(offsets[0].dot(tangent) < 0.0 && offsets[1].dot(tangent) > 0.0);
            assert_eq!(towers[0].diameter(), spec.tower_diameter);
            assert_eq!(towers[1].diameter(), spec.tower_diameter);
            let walks = [WallWalk::Linear {
                start: wall.start,
                end: wall.end,
                elevation_metres: wall.height_metres,
                width_metres: 1.25,
                outward: wall.outward,
            }];
            let defense = derive_gate_defenses(&program, &[spec], &towers, &[wall], &walks)
                .pop()
                .unwrap();
            let chamber = defense.guard_chamber;
            assert!((chamber.centre - threshold).length() < 0.001);
            assert!((chamber.size.dot(outward.abs()) - spec.chamber_depth.metres()).abs() < 0.001);
            assert_eq!(chamber.access.door.facing, wall.outward.opposite());
            assert!(
                (chamber.access.flight.bottom - chamber.access.flight.top)
                    .normalize_or_zero()
                    .dot(tangent)
                    > 0.99
            );
            assert!(
                (chamber.access.top_landing.centre - threshold).dot(-outward)
                    > spec.chamber_depth.metres() * 0.5
            );
            assert!(
                (chamber.access.bottom_landing.centre - threshold).dot(-outward)
                    > spec.chamber_depth.metres() * 0.5
            );
            assert!(
                (chamber.access.door.threshold_elevation_metres - chamber.floor_elevation_metres)
                    .abs()
                    < 0.001
            );
            assert_eq!(chamber.access.landing_guards.len(), 4);
            let top_end_mid = (chamber.access.landing_guards[1].start
                + chamber.access.landing_guards[1].end)
                * 0.5;
            let bottom_end_mid = (chamber.access.landing_guards[3].start
                + chamber.access.landing_guards[3].end)
                * 0.5;
            assert!((top_end_mid - chamber.access.top_landing.centre).dot(tangent) < -0.49);
            assert!((bottom_end_mid - chamber.access.bottom_landing.centre).dot(tangent) > 0.49);
            assert_eq!(chamber.access.lateral_braces.len(), 6);
            assert!(
                chamber
                    .access
                    .lateral_braces
                    .iter()
                    .filter(|brace| (brace.end - brace.start).dot(-outward).abs() >= 0.7)
                    .count()
                    >= 4
            );
            assert!(
                chamber
                    .access
                    .lateral_braces
                    .iter()
                    .filter(|brace| (brace.end - brace.start).dot(tangent).abs() >= 2.0)
                    .count()
                    >= 2
            );
            assert!((chamber.openings[0].position - threshold).dot(outward) > 0.0);
            assert!((defense.approach - threshold).dot(outward) > 5.9);
        }
        let diagonal = CurtainWallRun {
            start: Vec2::ZERO,
            end: Vec2::splat(12.0),
            height_metres: 6.0,
            thickness_metres: 1.2,
            outward: Direction::South,
            gate_width_metres: Some(3.2),
            gate_height_metres: 3.6,
        };
        assert!(resolve_gatehouse_towers(spec, diagonal, 6.0).is_none());
        let mismatched = CurtainWallRun {
            start: Vec2::new(-11.25, 0.0),
            end: Vec2::new(12.75, 0.0),
            outward: Direction::East,
            ..diagonal
        };
        assert!(resolve_gatehouse_towers(spec, mismatched, 6.0).is_none());
    }

    #[test]
    fn cathedral_has_independent_roof_slopes_and_a_bell_tower() {
        let plan = generate(&BuildingProgram::fixture(BuildingArchetype::Cathedral, 43)).unwrap();
        let pitches = plan
            .roofs
            .iter()
            .map(|roof| roof.pitch_degrees.round() as i32)
            .collect::<HashSet<_>>();
        assert!(pitches.len() >= 2);
        assert!(plan.square_towers.iter().any(|tower| tower.bell_openings));
        assert!(
            plan.square_towers
                .iter()
                .all(|tower| tower.roof.pitch_degrees > 60.0)
        );
        let principal_windows = plan.opening_assemblies.iter().filter(|opening| {
            opening.use_kind == crate::OpeningUse::Window
                && matches!(opening.profile, crate::OpeningProfile::PointedTwoCentred { width_metres, apex_height_metres, .. } if width_metres >= 0.9 && apex_height_metres >= 4.4)
        }).count();
        assert!(principal_windows >= 8);
        assert!(
            plan.resolved_geometry
                .solids
                .iter()
                .filter(|solid| solid.role == SolidRole::Mullion)
                .count()
                >= principal_windows * 2
        );
        let bell_openings = plan
            .opening_assemblies
            .iter()
            .filter(|opening| opening.use_kind == crate::OpeningUse::BellOpening)
            .collect::<Vec<_>>();
        assert_eq!(bell_openings.len(), 8);
        assert!(bell_openings.iter().all(|opening| matches!(
            opening.host_source,
            crate::WallSourceId::SquareTowerFace { .. }
        ) && opening.closure.layers
            == [crate::ClosureKind::TimberLouvre]));
    }
}
