fn derive_battlements(program: &BuildingProgram) -> Vec<BattlementRun> {
    let (width, depth) = program.footprint.dimensions();
    let width = f32::from(width) * CELL_SIZE_METRES;
    let depth = f32::from(depth) * CELL_SIZE_METRES;
    let top = program.storeys.len() as f32 * program.storey_height_metres;
    match program.archetype {
        BuildingArchetype::CastleGatehouse => {
            let covered_walk = BattlementRun {
                start: Vec2::new(width, 0.0),
                end: Vec2::new(width, depth),
                base_height_metres: top,
                kind: BattlementKind::CoveredWallWalk,
                outward: Direction::East,
            };
            let study = program.seed % 1_000;
            let mut runs = vec![covered_walk];
            if matches!(study, 201..=203) {
                // Isolated projected-defense studies retain the accepted
                // ordinary north/south crown and fighting circuit. Only the
                // named threatened point receives the studied installation.
                runs.extend([
                    BattlementRun {
                        start: Vec2::new(1.8, 0.0),
                        end: Vec2::new(width - 1.8, 0.0),
                        base_height_metres: top,
                        kind: BattlementKind::Crenellated,
                        outward: Direction::South,
                    },
                    BattlementRun {
                        start: Vec2::new(1.0, depth),
                        end: Vec2::new(width - 1.0, depth),
                        base_height_metres: top,
                        kind: BattlementKind::Crenellated,
                        outward: Direction::North,
                    },
                ]);
            }
            match study {
                201 => runs.push(BattlementRun {
                    start: Vec2::new(width + 0.08, depth * 0.36),
                    end: Vec2::new(width + 0.08, depth * 0.64),
                    base_height_metres: top,
                    kind: BattlementKind::Breteche,
                    outward: Direction::East,
                }),
                202 => runs.push(BattlementRun {
                    start: Vec2::new(0.0, 1.8),
                    end: Vec2::new(0.0, depth - 1.0),
                    base_height_metres: top,
                    kind: BattlementKind::RoofedHoarding,
                    outward: Direction::West,
                }),
                203 => {}
                _ => {
                    runs.push(BattlementRun {
                        start: Vec2::new(1.8, 0.0),
                        end: Vec2::new(width - 1.8, 0.0),
                        base_height_metres: top,
                        kind: BattlementKind::Machicolated,
                        outward: Direction::South,
                    });
                    runs.push(BattlementRun {
                        start: Vec2::new(1.0, depth),
                        end: Vec2::new(width - 1.0, depth),
                        base_height_metres: top,
                        kind: BattlementKind::OpenHoarding,
                        outward: Direction::North,
                    });
                }
            }
            runs
        }
        BuildingArchetype::CourtyardCastle => vec![
            BattlementRun {
                start: Vec2::new(0.0, 0.0),
                end: Vec2::new(width, 0.0),
                base_height_metres: top,
                kind: BattlementKind::Crenellated,
                outward: Direction::South,
            },
            BattlementRun {
                start: Vec2::new(0.0, 0.0),
                end: Vec2::new(0.0, depth),
                base_height_metres: top,
                kind: BattlementKind::Crenellated,
                outward: Direction::West,
            },
            BattlementRun {
                start: Vec2::new(width, 0.0),
                end: Vec2::new(width, depth),
                base_height_metres: top,
                kind: BattlementKind::Crenellated,
                outward: Direction::East,
            },
            BattlementRun {
                start: Vec2::new(0.0, depth),
                end: Vec2::new(width, depth),
                base_height_metres: top,
                kind: BattlementKind::Crenellated,
                outward: Direction::North,
            },
        ],
        BuildingArchetype::WalledKeep => {
            let margin = 9.0;
            let min = Vec2::splat(-margin);
            let max = Vec2::new(width + margin, depth + margin);
            let curtain_top = 6.0;
            let mut runs =
                rectangle_battlements(min, max, curtain_top, BattlementKind::Crenellated);
            runs.extend(rectangle_battlements(
                Vec2::ZERO,
                Vec2::new(width, depth),
                top,
                BattlementKind::Crenellated,
            ));
            runs
        }
        _ => Vec::new(),
    }
}

fn rectangle_battlements(
    min: Vec2,
    max: Vec2,
    height: f32,
    kind: BattlementKind,
) -> Vec<BattlementRun> {
    vec![
        BattlementRun {
            start: min,
            end: Vec2::new(max.x, min.y),
            base_height_metres: height,
            kind,
            outward: Direction::South,
        },
        BattlementRun {
            start: Vec2::new(max.x, min.y),
            end: max,
            base_height_metres: height,
            kind,
            outward: Direction::East,
        },
        BattlementRun {
            start: Vec2::new(min.x, max.y),
            end: max,
            base_height_metres: height,
            kind,
            outward: Direction::North,
        },
        BattlementRun {
            start: min,
            end: Vec2::new(min.x, max.y),
            base_height_metres: height,
            kind,
            outward: Direction::West,
        },
    ]
}

fn derive_crowns(
    program: &BuildingProgram,
    battlements: &[BattlementRun],
    towers: &[RoundTower],
) -> Vec<CrownAssembly> {
    if !matches!(
        program.archetype,
        BuildingArchetype::CourtyardCastle | BuildingArchetype::WalledKeep
    ) {
        return Vec::new();
    }
    // These are project/gameplay gates, not universal historical dimensions.
    // Pierced merlons are deliberately migrated as ordinary crenellation until
    // the resolved-void layer can prove a true through-piercing.
    let profile = CrownProfile {
        breastwork_height_metres: 0.9,
        merlon_height_metres: 0.72,
        thickness_metres: 0.45,
        merlon_width_metres: 0.72,
        crenel_width_metres: 0.48,
        coping_height_metres: 0.08,
        inner_guard_height_metres: 1.05,
        walk_clear_width_metres: 0.95,
        stance_height_metres: 0.0,
        firing_height_metres: 1.18,
        drain_spacing_metres: 3.6,
        inner_edge: InnerEdgeTreatment::MasonryUpstand,
    };
    let mut crowns = Vec::new();
    for run in battlements
        .iter()
        .filter(|run| run.kind == BattlementKind::Crenellated)
    {
        let owner = GeometryOwnerId(crowns.len() as u32 + 1);
        let length = (run.end - run.start).length();
        let tangent = (run.end - run.start).normalize_or_zero();
        let drains = (1..=((length / profile.drain_spacing_metres).floor() as usize).max(1))
            .map(|index| {
                run.start
                    + tangent * length * index as f32
                        / (((length / profile.drain_spacing_metres).floor() as usize).max(1) + 1)
                            as f32
            })
            .collect();
        crowns.push(CrownAssembly {
            owner,
            path: CrownPath::Straight {
                start: run.start,
                end: run.end,
                outward: run.outward,
            },
            base_height_metres: run.base_height_metres,
            profile,
            material: CrownMaterial::Masonry,
            phase: CrownPhase::PermanentMainWork,
            pattern: CrownPattern::Crenellated,
            junctions: Vec::new(),
            drain_positions: drains,
        });
    }
    for (tower_index, tower) in towers.iter().copied().enumerate() {
        let Some(kind) = tower.battlement else {
            continue;
        };
        if kind != BattlementKind::Crenellated {
            continue;
        }
        let owner = GeometryOwnerId(crowns.len() as u32 + 1);
        crowns.push(CrownAssembly {
            owner,
            path: CrownPath::Round {
                tower_index,
                centre: tower.centre_metres(),
                radius_metres: tower.radius_metres(),
            },
            base_height_metres: tower.wall_height_metres,
            profile,
            material: CrownMaterial::Masonry,
            phase: CrownPhase::PermanentMainWork,
            pattern: CrownPattern::Crenellated,
            junctions: Vec::new(),
            drain_positions: (0..8)
                .map(|index| {
                    let angle = index as f32 * std::f32::consts::TAU / 8.0;
                    tower.centre_metres()
                        + Vec2::new(angle.cos(), angle.sin()) * tower.radius_metres()
                })
                .collect(),
        });
    }
    let snapshot = crowns.clone();
    for crown in &mut crowns {
        let endpoints = match crown.path {
            CrownPath::Straight { start, end, .. } => vec![start, end],
            CrownPath::Round { .. } => Vec::new(),
        };
        for position in endpoints {
            let tower_match = snapshot.iter().find(|other| {
                if let CrownPath::Round {
                    centre,
                    radius_metres,
                    ..
                } = other.path
                {
                    other.owner != crown.owner
                        && (position - centre).length() <= radius_metres + 0.08
                } else {
                    false
                }
            });
            let other = tower_match.or_else(|| {
                snapshot.iter().find(|other| {
                    other.owner != crown.owner
                        && matches!(other.path, CrownPath::Straight { start, end, .. }
                            if (position - start).length() < 0.02 || (position - end).length() < 0.02)
                })
            });
            if let Some(other) = other {
                crown.junctions.push(CrownJunction {
                    owner: crown.owner,
                    other_owner: other.owner,
                    position,
                    kind: if matches!(other.path, CrownPath::Round { .. }) {
                        CrownJunctionKind::TowerSplice
                    } else {
                        CrownJunctionKind::Corner
                    },
                    clear_width_metres: profile.walk_clear_width_metres,
                });
            }
        }
        if let CrownPath::Straight { start, end, .. } = crown.path {
            let delta = end - start;
            for other in &snapshot {
                let CrownPath::Round {
                    centre,
                    radius_metres,
                    ..
                } = other.path
                else {
                    continue;
                };
                let progress =
                    ((centre - start).dot(delta) / delta.length_squared()).clamp(0.0, 1.0);
                let closest = start + delta * progress;
                if other.owner != crown.owner
                    && (closest - centre).length() <= radius_metres + 0.08
                    && !crown
                        .junctions
                        .iter()
                        .any(|junction| junction.other_owner == other.owner)
                {
                    crown.junctions.push(CrownJunction {
                        owner: crown.owner,
                        other_owner: other.owner,
                        position: closest,
                        kind: CrownJunctionKind::TowerSplice,
                        clear_width_metres: profile.walk_clear_width_metres,
                    });
                }
            }
        }
    }
    for crown in &mut crowns {
        let CrownPath::Round {
            centre,
            radius_metres,
            ..
        } = crown.path
        else {
            continue;
        };
        let owner = crown.owner;
        let links = snapshot
            .iter()
            .filter_map(|other| match other.path {
                CrownPath::Straight { start, end, .. } => {
                    let delta = end - start;
                    let progress =
                        ((centre - start).dot(delta) / delta.length_squared()).clamp(0.0, 1.0);
                    let closest = start + delta * progress;
                    ((closest - centre).length() <= radius_metres + 0.08)
                        .then_some((other, closest))
                }
                CrownPath::Round { .. } => None,
            })
            .map(|(other, position)| CrownJunction {
                owner,
                other_owner: other.owner,
                position,
                kind: CrownJunctionKind::TowerSplice,
                clear_width_metres: profile.walk_clear_width_metres,
            })
            .collect();
        crown.junctions = links;
    }
    crowns
}
