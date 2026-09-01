fn setup(
    world: &mut World,
    plan: &BuildingPlan,
    view: ViewerView,
    projected_kind: ProjectedProofKind,
    roof_proof: Option<RoofProofView>,
    scene_setup: SceneSetup,
) {
    let palette = create_palette(world);
    let dimensions = plan.dimensions_metres();
    let origin = Vec2::new(-dimensions.x * 0.5, -dimensions.y * 0.5);
    let storey_height = plan.storey_height_metres;
    let crown_proof = matches!(
        view,
        ViewerView::CrownStraightExterior
            | ViewerView::CrownStraightInterior
            | ViewerView::CrownCornerExterior
            | ViewerView::CrownCornerInterior
            | ViewerView::CrownTowerExterior
            | ViewerView::CrownTowerTop
            | ViewerView::CrownTowerCutaway
    );
    let projected_proof = projected_view(view);
    let mut removed_roof_items = roof_proof
        .filter(|proof| roof_proof_sectioned(*proof))
        .map(|proof| {
            roof_proof_assembly_indices(plan, proof)
                .into_iter()
                .filter_map(|index| {
                    plan.roof_assemblies[index]
                        .faces
                        .last()
                        .map(|face| face.id.0)
                })
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    removed_roof_items.extend(church_section_removed_roof_item_ids(plan, view));
    let calibrated_roof_ids = roof_proof
        .map(|proof| {
            roof_proof_assembly_indices(plan, proof)
                .into_iter()
                .map(|index| plan.roof_assemblies[index].id)
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    let architectural_proof = architectural_proof(view);
    let artillery_proof = artillery_proof_slug(view).is_some();
    let focused_ids = if architectural_proof {
        architectural_focus_item_ids(plan, view)
    } else if projected_proof {
        focused_projected_item_ids(plan, view, projected_kind)
    } else if artillery_proof {
        artillery_focus_item_ids(plan, view)
    } else {
        focused_crown_item_ids(plan, view)
    }
    .into_iter()
    .collect::<std::collections::HashSet<_>>();
    let mut proof_owners = plan
        .resolved_geometry
        .solids
        .iter()
        .filter(|solid| focused_ids.contains(&solid.id.0))
        .map(|solid| solid.owner.0)
        .collect::<std::collections::HashSet<_>>();
    if matches!(
        view,
        ViewerView::CrownTowerExterior | ViewerView::CrownTowerTop | ViewerView::CrownTowerCutaway
    ) {
        let adjacent = plan
            .crowns
            .iter()
            .filter(|crown| proof_owners.contains(&crown.owner.0))
            .flat_map(|crown| {
                crown
                    .junctions
                    .iter()
                    .map(|junction| junction.other_owner.0)
            })
            .collect::<Vec<_>>();
        proof_owners.extend(adjacent);
    }
    let proof_crown_matches_point = |point: Vec2| {
        plan.crowns.iter().any(|crown| {
            if !proof_owners.contains(&crown.owner.0) {
                return false;
            }
            match crown.path {
                CrownPath::Straight { start, end, .. } => {
                    let delta = end - start;
                    let progress =
                        ((point - start).dot(delta) / delta.length_squared()).clamp(0.0, 1.0);
                    point.distance(start + delta * progress) <= CELL_SIZE_METRES * 0.55
                }
                CrownPath::Round { .. } => false,
            }
        })
    };

    let scene_span = plan.towers.iter().fold(dimensions.length(), |span, tower| {
        let position = tower.centre_metres() + origin;
        span.max((position.abs() + Vec2::splat(tower.radius_metres())).length() * 2.0)
    });
    if scene_setup != SceneSetup::EditorBuilding {
        if scene_setup == SceneSetup::EditorInitial {
            spawn_ground(world, Vec2::splat(100.0), false);
        } else if plan.artillery_castle.is_some() {
            spawn_artillery_ground(world, Vec2::splat(scene_span), origin);
        } else {
            spawn_ground(
                world,
                Vec2::splat(scene_span),
                crown_proof || architectural_proof,
            );
        }
    }
    for storey in &plan.storeys {
        if projected_proof
            || architectural_proof
            || timber_isolated_view(view)
            || artillery_isolated_view(view)
        {
            continue;
        }
        if matches!(view, ViewerView::Cutaway | ViewerView::TowerPortalDetail) && storey.level > 0 {
            continue;
        }
        let base_y = f32::from(storey.level) * storey_height;
        for room in &storey.rooms {
            if crown_proof {
                continue;
            }
            let floor_material =
                if matches!(view, ViewerView::Cutaway | ViewerView::TowerPortalDetail) {
                    &palette.room_floors[usize::from(room.id) % palette.room_floors.len()]
                } else {
                    &palette.floor
                };
            for cell in &room.cells {
                spawn_box(
                    world,
                    floor_material,
                    Vec3::new(CELL_SIZE_METRES - 0.04, 0.12, CELL_SIZE_METRES - 0.04),
                    Vec3::new(
                        cell.centre().x + origin.x,
                        base_y + 0.06,
                        cell.centre().y + origin.y,
                    ),
                    Quat::IDENTITY,
                    "room floor",
                );
            }
        }
        for (wall_index, wall) in storey.walls.iter().copied().enumerate() {
            if crown_proof && !proof_crown_matches_point(wall.centre()) {
                continue;
            }
            if matches!(view, ViewerView::Cutaway | ViewerView::TowerPortalDetail)
                && wall.exterior()
                && matches!(wall.direction, Direction::South | Direction::East)
            {
                continue;
            }
            let resolved_host_replaces_wall = (plan.timber_frame.is_some()
                || !matches!(
                    view,
                    ViewerView::Cutaway
                        | ViewerView::TowerPortalDetail
                        | ViewerView::GateDetailInterior
                ))
                && plan.wall_assemblies.iter().any(|assembly| {
                    matches!(
                        assembly.source,
                        adventuresim_building_generator::WallSourceId::StoreyWall {
                            storey_level,
                            wall_index: source_wall,
                        } if storey_level == storey.level && source_wall == wall_index
                    )
                });
            if resolved_host_replaces_wall {
                continue;
            }
            let opening = storey
                .openings
                .iter()
                .find(|opening| opening.wall == wall_index);
            spawn_wall(
                world,
                &palette,
                wall,
                opening,
                origin,
                base_y,
                storey_height,
                plan.wall_style,
                InteriorWallFinish::Plastered,
                plan.timber_frame_style,
                plan.upper_storey_projection_metres * f32::from(storey.level),
            );
        }
        let projection = plan.upper_storey_projection_metres * f32::from(storey.level);
        if plan.timber_frame.is_none()
            && !architectural_proof
            && plan.wall_style == WallStyle::TimberFrame
            && projection > 0.01
        {
            let min_x = origin.x - projection;
            let max_x = origin.x + dimensions.x + projection;
            let min_z = origin.y - projection;
            let max_z = origin.y + dimensions.y + projection;
            for z in [min_z, max_z] {
                spawn_box(
                    world,
                    &palette.timber,
                    Vec3::new(max_x - min_x, 0.14, 0.16),
                    Vec3::new((min_x + max_x) * 0.5, base_y + 0.04, z),
                    Quat::IDENTITY,
                    "projecting storey sill",
                );
            }
            for x in [min_x, max_x] {
                spawn_box(
                    world,
                    &palette.timber,
                    Vec3::new(0.16, 0.14, max_z - min_z),
                    Vec3::new(x, base_y + 0.04, (min_z + max_z) * 0.5),
                    Quat::IDENTITY,
                    "projecting storey sill",
                );
                for z in [min_z, max_z] {
                    spawn_box(
                        world,
                        &palette.timber,
                        Vec3::new(0.18, storey_height, 0.18),
                        Vec3::new(x, base_y + storey_height * 0.5, z),
                        Quat::IDENTITY,
                        "projecting storey corner post",
                    );
                }
            }
        }
    }

    if !projected_proof
        && !architectural_proof
        && (!timber_isolated_view(view) || view == ViewerView::TimberGableRoofBearing)
        && !artillery_isolated_view(view)
        && !matches!(
            view,
            ViewerView::Cutaway
                | ViewerView::TowerPortalDetail
                | ViewerView::CrownStraightExterior
                | ViewerView::CrownStraightInterior
                | ViewerView::CrownCornerExterior
                | ViewerView::CrownCornerInterior
                | ViewerView::CrownTowerExterior
                | ViewerView::CrownTowerTop
                | ViewerView::CrownTowerCutaway
        )
    {
        for roof in &plan.roof_assemblies {
            spawn_resolved_roof(
                world,
                &palette,
                roof,
                &plan.resolved_geometry,
                origin,
                &removed_roof_items,
                calibrated_roof_ids.contains(&roof.id),
                view == ViewerView::TimberGableRoofBearing,
            );
        }
    }
    for (tower_index, tower) in plan.towers.iter().copied().enumerate() {
        if view == ViewerView::ArtilleryCurtainSection
            || (view == ViewerView::ArtilleryRondelCasemate && tower_index != 0)
        {
            continue;
        }
        if projected_proof
            || (architectural_proof && view != ViewerView::WallRoundTowerRadialSection)
            || (view == ViewerView::WallRoundTowerRadialSection && tower_index != 0)
        {
            continue;
        }
        if crown_proof
            && !plan.crowns.iter().any(|crown| {
                proof_owners.contains(&crown.owner.0)
                    && matches!(crown.path, CrownPath::Round { tower_index: index, .. } if index == tower_index)
            })
        {
            continue;
        }
        if view == ViewerView::TowerPortalDetail && tower_index != 0 {
            continue;
        }
        if view == ViewerView::GateDetailInterior {
            // Bailey-side section: the exterior preset proves both flanking
            // towers. Remove their shells here so the chamber route and two
            // closure planes remain directly inspectable.
            continue;
        }
        let portals = if view == ViewerView::WallRoundTowerRadialSection {
            Vec::new()
        } else {
            plan.tower_portals
                .iter()
                .copied()
                .filter(|portal| portal.tower_index == tower_index)
                .collect::<Vec<_>>()
        };
        let mut firing_positions = if view == ViewerView::WallRoundTowerRadialSection {
            Vec::new()
        } else {
            plan.gate_defenses
                .iter()
                .flat_map(|gate| gate.firing_positions.iter().copied())
                .filter(|position| position.tower_index == tower_index)
                .collect::<Vec<_>>()
        };
        if let Some(castle) = &plan.artillery_castle {
            firing_positions.extend(
                castle
                    .stations
                    .iter()
                    .filter(|station| station.rondel.0 as usize == tower_index)
                    .filter_map(|station| {
                        let opening = plan
                            .opening_assemblies
                            .iter()
                            .find(|opening| opening.id == station.opening)?;
                        let exterior_height = match opening.profile {
                            adventuresim_building_generator::OpeningProfile::GunLoop {
                                exterior_height_metres,
                                ..
                            } => exterior_height_metres,
                            _ => return None,
                        };
                        Some(FiringPosition {
                            aperture_id: station.id.0 as u16,
                            tower_index,
                            origin: opening.frame.origin,
                            aperture_normal: station.facing,
                            direction: station.facing,
                            elevation_metres: opening.sill_elevation_metres + exterior_height * 0.5,
                            range_metres: 24.0,
                            half_arc_degrees: 38.0,
                            aperture_width_metres: opening.profile.exterior_width_metres(),
                        })
                    }),
            );
        }
        spawn_tower(
            world,
            &palette,
            plan,
            tower_index,
            tower,
            origin,
            view,
            &portals,
            &firing_positions,
            plan.crowns.iter().any(|crown| matches!(crown.path, CrownPath::Round { tower_index: index, .. } if index == tower_index)),
        );
    }
    for tower in plan.square_towers.iter().copied() {
        if projected_proof || architectural_proof || plan.church.is_some() {
            continue;
        }
        spawn_square_tower(world, &palette, tower, origin, view);
    }
    for stair in plan.stairs.iter().copied() {
        // The programme renderer already draws the timber resolver's flight
        // from resolved geometry. The semantic recipe remains for detached
        // editing, but rendering it here would duplicate that same stair.
        if plan.timber_frame.is_some() {
            continue;
        }
        if timber_isolated_view(view) {
            continue;
        }
        if matches!(view, ViewerView::ArtilleryCurtainSection | ViewerView::ArtilleryGateInterior)
            // The authoritative resolved ArtilleryStairTread solids already
            // supply the lower casemate flight and are section-filtered above
            // 3.05 m.  The legacy whole-height stair duplicated them and hid
            // the working stations this proof is required to expose.
            || view == ViewerView::ArtilleryRondelCasemate
        {
            continue;
        }
        if plan.church.is_some() {
            // Church service stairs are resolved solids so circulation audit,
            // correspondence, and rendering share one geometry authority.
            continue;
        }
        if projected_proof || architectural_proof {
            continue;
        }
        if view == ViewerView::GateDetailInterior {
            continue;
        }
        if crown_proof {
            let centre = match stair {
                Stair::Spiral { centre, .. } => centre,
                Stair::Straight { start, .. } => start,
            };
            if !plan.crowns.iter().any(|crown| {
                proof_owners.contains(&crown.owner.0)
                    && matches!(crown.path, CrownPath::Round { centre: tower, .. } if tower.distance(centre) < 0.02)
            }) {
                continue;
            }
        }
        spawn_stair(world, &palette, stair, origin);
    }
    for (walk_index, mut wall_walk) in plan.wall_walks.iter().copied().enumerate() {
        if projected_proof || architectural_proof {
            continue;
        }
        let resolved_by_accepted_crown =
            plan.crowns
                .iter()
                .any(|crown| match (crown.path, wall_walk) {
                    (
                        CrownPath::Straight { start, end, .. },
                        WallWalk::Linear {
                            start: walk_start,
                            end: walk_end,
                            ..
                        },
                    ) => {
                        (start.distance(walk_start) < 0.02 && end.distance(walk_end) < 0.02)
                            || (start.distance(walk_end) < 0.02 && end.distance(walk_start) < 0.02)
                    }
                    (
                        CrownPath::Round { centre, .. },
                        WallWalk::Round {
                            centre: walk_centre,
                            ..
                        },
                    ) => centre.distance(walk_centre) < 0.02,
                    _ => false,
                });
        if resolved_by_accepted_crown && view != ViewerView::GateDetailInterior {
            continue;
        }
        if crown_proof
            && !plan.crowns.iter().any(|crown| {
                if !proof_owners.contains(&crown.owner.0) {
                    return false;
                }
                match (crown.path, wall_walk) {
                    (
                        CrownPath::Straight { start, end, .. },
                        WallWalk::Linear {
                            start: walk_start,
                            end: walk_end,
                            ..
                        },
                    ) => {
                        (start.distance(walk_start) < 0.02 && end.distance(walk_end) < 0.02)
                            || (start.distance(walk_end) < 0.02 && end.distance(walk_start) < 0.02)
                    }
                    (
                        CrownPath::Round { centre, .. },
                        WallWalk::Round {
                            centre: walk_centre,
                            ..
                        },
                    ) => centre.distance(walk_centre) < 0.02,
                    _ => false,
                }
            })
        {
            continue;
        }
        if view == ViewerView::GateDetailInterior && !matches!(wall_walk, WallWalk::Linear { .. }) {
            continue;
        }
        if view == ViewerView::GateDetailInterior {
            let Some(defense) = plan.gate_defenses.first() else {
                continue;
            };
            let access = &defense.guard_chamber.access;
            if walk_index != access.from_walk_index {
                continue;
            }
            if let WallWalk::Linear {
                start,
                end,
                elevation_metres,
                width_metres,
                outward,
            } = wall_walk
            {
                // The section preset needs enough rampart to prove positive
                // landing contact, but a full curtain-length slab masks the
                // chamber machinery from the bailey-side camera.
                let tangent = (end - start).normalize_or_zero();
                let projected = start + tangent * (access.top_landing.centre - start).dot(tangent);
                wall_walk = WallWalk::Linear {
                    start: projected - tangent * 2.0,
                    end: projected + tangent * 2.0,
                    elevation_metres,
                    width_metres,
                    outward,
                };
            }
        }
        spawn_wall_walk(world, &palette, wall_walk, origin);
    }
    if !matches!(view, ViewerView::Cutaway | ViewerView::TowerPortalDetail) {
        for (wall_index, curtain_wall) in plan.curtain_walls.iter().copied().enumerate() {
            if projected_proof || architectural_proof {
                continue;
            }
            if crown_proof
                && !plan.crowns.iter().any(|crown| {
                    proof_owners.contains(&crown.owner.0)
                        && matches!(crown.path, CrownPath::Straight { start, end, .. }
                            if ((start-curtain_wall.start).perp_dot(end-start)).abs() < 0.05
                                && ((end-curtain_wall.start).perp_dot(curtain_wall.end-curtain_wall.start)).abs() < 0.05)
                })
            {
                continue;
            }
            let closures = plan
                .gate_defenses
                .iter()
                .flat_map(|gate| gate.closures.iter().copied())
                .filter(|closure| closure.curtain_wall_index == wall_index)
                .collect::<Vec<_>>();
            if let Some(defense) = plan
                .gate_defenses
                .iter()
                .find(|defense| defense.curtain_wall_index == wall_index)
            {
                spawn_gatehouse_curtain(
                    world,
                    &palette,
                    curtain_wall,
                    defense,
                    &plan.towers,
                    origin,
                );
            } else {
                spawn_curtain_wall(world, &palette, curtain_wall, origin, &closures);
            }
        }
        if view != ViewerView::GateDetailInterior {
            spawn_resolved_crowns(
                world,
                &palette,
                plan,
                origin,
                (crown_proof
                    || projected_proof
                    || artillery_proof
                    || (architectural_proof && timber_proof_suffix(view).is_none()))
                .then_some(&proof_owners),
                (section_proof(view)
                    || church_section_proof(view)
                    || church_proof_slug(view).is_some()
                    || timber_proof_suffix(view).is_some()
                    || artillery_proof_slug(view).is_some())
                .then_some(view),
            );
            if architectural_proof || timber_proof_suffix(view).is_some() {
                spawn_resolved_architectural_surfaces(
                    world,
                    &palette,
                    plan,
                    origin,
                    &proof_owners,
                    view,
                );
            }
            if projected_proof
                && let Some(defense) = focused_projected_defense(plan, view, projected_kind)
            {
                spawn_projected_proof_markers(world, &palette, plan, defense.owner, origin, view);
            }
            if matches!(
                view,
                ViewerView::CrownStraightExterior
                    | ViewerView::CrownStraightInterior
                    | ViewerView::CrownCornerExterior
                    | ViewerView::CrownCornerInterior
                    | ViewerView::CrownTowerExterior
                    | ViewerView::CrownTowerTop
                    | ViewerView::CrownTowerCutaway
            ) {
                spawn_crown_defender_scale(world, &palette, plan, view, origin);
            }
            for run in plan.battlements.iter().copied() {
                if crown_proof || projected_proof || architectural_proof {
                    continue;
                }
                if !plan.crowns.is_empty()
                    && matches!(
                        run.kind,
                        BattlementKind::Crenellated
                            | BattlementKind::PiercedCrenellated
                            | BattlementKind::GunLoopParapet
                    )
                {
                    continue;
                }
                if matches!(
                    run.kind,
                    BattlementKind::Machicolated
                        | BattlementKind::Breteche
                        | BattlementKind::OpenHoarding
                        | BattlementKind::RoofedHoarding
                ) {
                    continue;
                }
                spawn_battlement_run(world, &palette, run, origin);
            }
        }
    }
    if view != ViewerView::TowerPortalDetail
        && !crown_proof
        && !projected_proof
        && !architectural_proof
    {
        for defense in &plan.gate_defenses {
            if let Some(wall) = plan.curtain_walls.get(defense.curtain_wall_index).copied() {
                spawn_gate_guard_chamber(world, &palette, defense, wall, origin, view);
            }
        }
    }
    if section_proof(view) || artillery_section_proof(view) {
        spawn_architectural_section_markers(world, &palette, plan, view, origin);
    }
    if artillery_proof_slug(view).is_some() {
        spawn_artillery_proof_markers(world, plan, view, origin);
        let annotation = plan.artillery_castle.as_ref().map_or_else(String::new, |castle| {
            format!(
                "target={} | artillery={} | phase={:?} | trace=orthogonal | curtains={} | rondels={} | stations={} | routes={} | fire={} | cut={:?}",
                artillery_proof_slug(view).unwrap_or_default(),
                castle.id.0,
                castle.phase,
                castle.curtains.len(),
                castle.rondels.len(),
                castle.stations.len(),
                castle.route_edges.len(),
                castle.stations.iter().map(|station| station.rays.len()).sum::<usize>(),
                artillery_cut_plane(view),
            )
        });
        world.spawn((
            Name::new("artillery proof authority annotation"),
            Text::new(annotation),
            TextFont {
                font_size: FontSize::Px(17.0),
                ..default()
            },
            TextColor(Color::srgb(0.06, 0.06, 0.05)),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(3.0),
                bottom: Val::Percent(3.0),
                ..default()
            },
            NonCollidingVisualization,
        ));
    }
    if timber_proof_suffix(view).is_some() {
        let annotation = plan.timber_frame.as_ref().map_or_else(String::new, |frame| {
            format!(
                "target={} | frame={} | program={:?} | roles={:?} | cut={:?} | members={} | joints={} | exact resolved IDs",
                timber_proof_slug(plan, view).unwrap_or_default(),
                frame.id.0,
                frame.program,
                timber_required_roles(plan, view),
                timber_cut_plane(plan, view),
                frame.members.len(),
                frame.joints.len(),
            )
        });
        world.spawn((
            Name::new("timber proof authority annotation"),
            Text::new(annotation),
            TextFont {
                font_size: FontSize::Px(17.0),
                ..default()
            },
            TextColor(Color::srgb(0.06, 0.06, 0.05)),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(3.0),
                bottom: Val::Percent(3.0),
                ..default()
            },
            NonCollidingVisualization,
        ));
    }
    if church_proof_slug(view).is_some() {
        let annotation = plan.church.as_ref().map_or_else(String::new, |church| {
            let opening_count = plan
                .opening_assemblies
                .iter()
                .filter(|opening| {
                    matches!(
                        opening.host_source,
                        adventuresim_building_generator::WallSourceId::ChurchExterior { .. }
                            | adventuresim_building_generator::WallSourceId::ChurchArcade { .. }
                            | adventuresim_building_generator::WallSourceId::ChurchApse { .. }
                            | adventuresim_building_generator::WallSourceId::ChurchTowerFace { .. }
                            | adventuresim_building_generator::WallSourceId::SquareTowerFace { .. }
                    )
                })
                .count();
            format!(
                "target={:?} | church={} | 3-aisled cruciform basilica | bays N1-N4 / X / Q1-Q2 / A5 | roles={:?} | cut={:?} | openings={} | supports={}",
                church_target_component_ids(plan, view),
                church.id.0,
                church_required_roles(view),
                church_cut_plane(plan, view),
                opening_count,
                plan.resolved_geometry.structural_nodes.len(),
            )
        });
        world.spawn((
            Name::new("church proof authority annotation"),
            Text::new(annotation),
            TextFont {
                font_size: FontSize::Px(17.0),
                ..default()
            },
            TextColor(Color::srgb(0.06, 0.06, 0.05)),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(3.0),
                bottom: Val::Percent(3.0),
                ..default()
            },
            NonCollidingVisualization,
        ));
    }
    if let Some(proof) = roof_proof {
        let indices = roof_proof_assembly_indices(plan, proof);
        let annotation = format!(
            "{}  roof_ids={:?}  faces={}  edges={}  cuts={}",
            roof_proof_slug(proof),
            indices
                .iter()
                .map(|index| plan.roof_assemblies[*index].id.0)
                .collect::<Vec<_>>(),
            indices
                .iter()
                .map(|index| plan.roof_assemblies[*index].faces.len())
                .sum::<usize>(),
            indices
                .iter()
                .map(|index| plan.roof_assemblies[*index].edges.len())
                .sum::<usize>(),
            indices
                .iter()
                .map(|index| plan.roof_assemblies[*index]
                    .faces
                    .iter()
                    .map(|face| face.cutouts.len())
                    .sum::<usize>())
                .sum::<usize>(),
        );
        world.spawn((
            Name::new("roof proof authority annotation"),
            Text::new(annotation),
            TextFont {
                font_size: FontSize::Px(22.0),
                ..default()
            },
            TextColor(Color::srgb(0.06, 0.06, 0.05)),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(3.0),
                bottom: Val::Percent(3.0),
                ..default()
            },
            NonCollidingVisualization,
        ));
    }

    let roof_height = plan
        .roofs
        .iter()
        .map(|roof| {
            let span = match roof.ridge_axis {
                RidgeAxis::Z => roof.size.x * 0.5 + roof.eave_metres,
                RidgeAxis::X => roof.size.y * 0.5 + roof.eave_metres,
            };
            roof.base_height_metres + span * roof.pitch_degrees.to_radians().tan()
        })
        .chain(plan.roof_dormers.iter().map(|dormer| {
            dormer.base_height_metres + dormer.height_metres + dormer.width_metres * 0.65
        }))
        .fold(0.0, f32::max);
    let max_height = plan
        .towers
        .iter()
        .map(|tower| tower.wall_height_metres + tower.radius_metres() * 1.8)
        .fold(
            (plan.storeys.len() as f32 * storey_height + 7.0).max(roof_height),
            f32::max,
        );
    let radius = scene_span.max(max_height) * 1.05;
    let target = Vec3::new(0.0, max_height * 0.35, 0.0);
    let roof_focus_indices = roof_proof
        .map(|proof| roof_proof_assembly_indices(plan, proof))
        .unwrap_or_default();
    let (mut roof_focus, mut roof_focus_extent) = if roof_focus_indices.is_empty() {
        (target, radius)
    } else {
        let (min, max) = roof_focus_indices
            .iter()
            .flat_map(|index| &plan.roof_assemblies[*index].faces)
            .flat_map(|face| &face.polygon)
            .map(|point| Vec3::new(point.x + origin.x, point.y, point.z + origin.y))
            .fold(
                (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                |(min, max), point| (min.min(point), max.max(point)),
            );
        ((min + max) * 0.5, (max - min).max_element().max(3.0))
    };
    if let Some(proof) = roof_proof {
        let slug = roof_proof_slug(proof);
        if slug.starts_with("roof-dormer-")
            && let Some(child_id) = plan
                .roof_assemblies
                .iter()
                .flat_map(|roof| &roof.children)
                .find(|child| {
                    matches!(
                        child.kind,
                        adventuresim_building_generator::RoofChildKind::GabledDormer
                            | adventuresim_building_generator::RoofChildKind::ShedDormer
                    )
                })
                .map(|child| child.child)
            && let Some(child) = plan.roof_assemblies.iter().find(|roof| roof.id == child_id)
        {
            // Dormer evidence is an assembly inspection, not a whole-roof
            // beauty shot.  Bound the camera to the exact child faces and
            // enclosure so gaps, projecting curbs, and oversized eave pieces
            // occupy enough pixels to be reviewable.
            let (min, max) = child
                .faces
                .iter()
                .flat_map(|face| face.polygon.iter())
                .chain(
                    child
                        .enclosure_faces
                        .iter()
                        .flat_map(|face| face.polygon.iter()),
                )
                .map(|point| Vec3::new(point.x + origin.x, point.y, point.z + origin.y))
                .fold(
                    (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                    |(min, max), point| (min.min(point), max.max(point)),
                );
            roof_focus = (min + max) * 0.5;
            roof_focus_extent = (max - min).max_element().max(2.2);
        } else if slug.starts_with("roof-abutment-tower-")
            && !slug.ends_with("-top")
            && !slug.ends_with("-drainage")
            && let Some(tower) = plan.square_towers.first()
        {
            roof_focus = Vec3::new(
                tower.centre.x + origin.x,
                tower.wall_height_metres - 6.0,
                tower.centre.y + origin.y,
            );
            roof_focus_extent = (tower.size.max_element() + 2.0).max(17.0);
        } else if slug.starts_with("roof-cross-gable-")
            && let Some(child_id) = plan
                .roof_assemblies
                .iter()
                .flat_map(|roof| &roof.children)
                .find(|child| {
                    child.kind == adventuresim_building_generator::RoofChildKind::CrossGable
                        && child.facade_wall.is_some()
                })
                .map(|child| child.child)
            && let Some(wall) = plan.wall_assemblies.iter().find(|wall| {
                wall.source
                    == adventuresim_building_generator::WallSourceId::RoofChildFront {
                        roof: child_id,
                    }
            })
        {
            roof_focus = Vec3::new(
                wall.frame.origin.x + origin.x,
                wall.base_elevation_metres + wall.height_metres * 0.55,
                wall.frame.origin.y + origin.y,
            );
            // The proof keeps the parent weather face and its real cut in
            // frame as context, so distance is governed by the host roof as
            // well as the narrower facade-derived child.
            roof_focus_extent = wall.length_metres.max(wall.height_metres).max(14.0);
        }
        if slug.starts_with("roof-round-tower-") || slug.starts_with("roof-pavilion-") {
            let owners = roof_focus_indices
                .iter()
                .map(|index| plan.roof_assemblies[*index].owner)
                .collect::<std::collections::HashSet<_>>();
            let downspouts = plan
                .resolved_geometry
                .roof_drainage_networks
                .iter()
                .filter(|network| owners.contains(&network.owner))
                .filter_map(|network| network.downspout)
                .collect::<std::collections::HashSet<_>>();
            let include_downspouts = slug.ends_with("-drainage");
            let bounds = plan
                .resolved_geometry
                .solids
                .iter()
                .filter(|solid| {
                    owners.contains(&solid.owner)
                        && matches!(
                            solid.role,
                            SolidRole::RoofFace
                                | SolidRole::RoofFraming
                                | SolidRole::RoofEdgeTreatment
                                | SolidRole::RoofFlashing
                                | SolidRole::RoofPlate
                                | SolidRole::RoofGutter
                        )
                        && (include_downspouts || !downspouts.contains(&solid.id))
                })
                .map(|solid| {
                    let cosine = solid.yaw_radians.cos().abs();
                    let sine = solid.yaw_radians.sin().abs();
                    let half = Vec3::new(
                        (solid.size.x * cosine + solid.size.z * sine) * 0.5,
                        (solid.size.y
                            + solid.size.x * solid.longfall_radians.sin().abs()
                            + solid.size.z * solid.crossfall_radians.sin().abs())
                            * 0.5,
                        (solid.size.x * sine + solid.size.z * cosine) * 0.5,
                    );
                    let centre = solid.centre + Vec3::new(origin.x, 0.0, origin.y);
                    (centre - half, centre + half)
                })
                .fold(None, |bounds, (min, max)| {
                    Some(
                        bounds.map_or((min, max), |(old_min, old_max): (Vec3, Vec3)| {
                            (old_min.min(min), old_max.max(max))
                        }),
                    )
                });
            if let Some((min, max)) = bounds {
                roof_focus = (min + max) * 0.5;
                roof_focus_extent = (max - min).max_element().max(roof_focus_extent);
            }
        }
    }
    let straight_crown_focus = plan
        .crowns
        .iter()
        .find_map(|crown| match crown.path {
            CrownPath::Straight { start, end, .. } => {
                Some(((start + end) * 0.5 + origin, crown.base_height_metres))
            }
            CrownPath::Round { .. } => None,
        })
        .unwrap_or((Vec2::ZERO, 6.0));
    let corner_crown_focus = plan
        .crowns
        .iter()
        .flat_map(|crown| {
            crown
                .junctions
                .iter()
                .map(move |junction| (crown, junction))
        })
        .find(|(_, junction)| {
            junction.kind == adventuresim_building_generator::CrownJunctionKind::Corner
        })
        .map(|(crown, junction)| (junction.position + origin, crown.base_height_metres))
        .unwrap_or(straight_crown_focus);
    let preferred_tower = plan
        .gate_defenses
        .first()
        .and_then(|gate| gate.firing_positions.first())
        .map(|position| position.tower_index);
    let tower_crown_focus = plan
        .crowns
        .iter()
        .find_map(|crown| match crown.path {
            CrownPath::Round {
                tower_index,
                centre,
                ..
            } if preferred_tower.is_none_or(|preferred| preferred == tower_index) => {
                Some((centre + origin, crown.base_height_metres))
            }
            CrownPath::Straight { .. } => None,
            CrownPath::Round { .. } => None,
        })
        .unwrap_or(straight_crown_focus);
    let (
        projected_focus,
        projected_outward,
        projected_tangent,
        projected_extent,
        projected_vertical_extent,
    ) = focused_projected_defense(plan, view, projected_kind)
        .map(|defense| {
            let (focus, outward, extent) = match defense.path {
                ProjectedDefensePath::Linear {
                    start,
                    end,
                    outward,
                } => (
                    (start + end) * 0.5 + origin,
                    direction_vector_2d(outward),
                    start.distance(end),
                ),
                ProjectedDefensePath::Round {
                    centre,
                    radius_metres,
                    outward,
                } => (
                    centre + origin,
                    direction_vector_2d(outward),
                    radius_metres * 2.0,
                ),
            };
            let (min_y, max_y) = plan
                .resolved_geometry
                .solids
                .iter()
                .filter(|solid| solid.owner == defense.owner || solid.owner == defense.host_owner)
                .fold(
                    (f32::INFINITY, f32::NEG_INFINITY),
                    |(min_y, max_y), solid| {
                        (
                            min_y.min(solid.centre.y - solid.size.y * 0.5),
                            max_y.max(solid.centre.y + solid.size.y * 0.5),
                        )
                    },
                );
            (
                Vec3::new(focus.x, (min_y + max_y) * 0.5, focus.y),
                outward,
                Vec2::new(-outward.y, outward.x),
                extent,
                max_y - min_y,
            )
        })
        .unwrap_or((Vec3::new(0.0, 7.0, 0.0), -Vec2::Y, Vec2::X, 6.0, 4.0));
    let projected_distance = (if projected_kind == ProjectedProofKind::Breteche {
        (projected_extent * 0.5 + 3.5)
            * if matches!(
                view,
                ViewerView::ProjectedInterior | ViewerView::ProjectedUnderside
            ) {
                1.3
            } else {
                1.45
            }
    } else if projected_extent < 3.0 {
        4.5
    } else {
        projected_extent * 0.5 + 3.5
    })
    .max(projected_vertical_extent * 0.65 + 2.0)
        * 1.25;
    let projected_flank_scale = if projected_extent < 3.0 { 1.35 } else { 1.0 };
    let projected_interior_scale = if projected_kind == ProjectedProofKind::Breteche {
        0.95
    } else if projected_extent < 3.0 {
        1.30
    } else {
        1.0
    };
    let projected_underside_scale = if projected_kind == ProjectedProofKind::Breteche {
        0.94
    } else {
        1.0
    };
    let projected_top_scale = if projected_kind == ProjectedProofKind::Breteche {
        0.94
    } else {
        1.0
    };
    let (architectural_focus, architectural_outward, architectural_tangent, architectural_distance) =
        if let Some(opening) = focused_opening(plan, view) {
            let height = opening.profile.clear_height_metres();
            let host = plan
                .wall_assemblies
                .iter()
                .find(|wall| wall.id == opening.host_wall);
            let focus_y = host.map_or(opening.sill_elevation_metres + height * 0.5, |wall| {
                wall.base_elevation_metres + wall.height_metres * 0.5
            });
            let proof_distance = host.map_or(height * 1.65 + 2.0, |wall| {
                (height * 1.65 + 2.0).max(wall.height_metres * 1.55 + 1.0)
            });
            (
                Vec3::new(
                    opening.frame.origin.x + origin.x,
                    focus_y,
                    opening.frame.origin.y + origin.y,
                ),
                opening.frame.outward,
                opening.frame.tangent,
                // Proof owners include the full load-bearing jamb/head assembly,
                // not only the blue/void opening.  Frame that structural height
                // with enough margin for section labels and the 1.75 m scale.
                proof_distance.max(6.2),
            )
        } else if view == ViewerView::WallRoundTowerRadialSection {
            let tower = plan.towers.first().copied();
            let centre = tower.map(RoundTower::centre_metres).unwrap_or(Vec2::ZERO) + origin;
            (
                Vec3::new(
                    centre.x,
                    tower.map_or(3.0, |tower| tower.wall_height_metres * 0.5),
                    centre.y,
                ),
                -Vec2::Y,
                Vec2::X,
                tower.map_or(9.0, |tower| tower.radius_metres() * 4.5),
            )
        } else if let Some(wall) = focused_wall(plan, view) {
            (
                Vec3::new(
                    wall.frame.origin.x + origin.x,
                    wall.base_elevation_metres + wall.height_metres * 0.5,
                    wall.frame.origin.y + origin.y,
                ),
                wall.frame.outward,
                wall.frame.tangent,
                (wall.height_metres * 1.5 + 0.8).max(5.4),
            )
        } else {
            (Vec3::ZERO, -Vec2::Y, Vec2::X, 5.0)
        };
    let church_camera = church_camera(plan, view, origin);
    let timber_camera = timber_camera(plan, view, origin);
    let artillery_camera = artillery_camera(plan, view, origin);
    let camera_position = if let Some((camera, _)) = artillery_camera {
        camera
    } else if let Some((camera, _)) = timber_camera {
        camera
    } else if let Some((camera, _)) = church_camera {
        camera
    } else if let Some(proof) = roof_proof {
        let slug = roof_proof_slug(proof);
        let distance_scale = if slug == "roof-courtyard-valleys-top"
            || matches!(slug, "roof-l-valley-top" | "roof-l-valley-drainage")
        {
            // Keep the complete four-wing courtyard footprint, including
            // its drainage terminals, inside the top-view proof frame.
            2.25
        } else if slug == "roof-l-valley-underside" {
            2.25
        } else if slug == "roof-pavilion-drainage" {
            0.80
        } else if slug == "roof-round-tower-drainage" {
            // Shared perimeter outlets no longer create a full-height pipe
            // cage. Frame the complete cap and all four outlet stations;
            // the prior pipe-oriented close crop clipped the high pavilion.
            1.25
        } else if slug.starts_with("roof-abutment-tower-") {
            // The high bell tower and its lower-corner outlet must both fit in
            // every proof without clipping the parent cut/contact contour.
            1.55
        } else if slug == "roof-dormer-gabled-interior" || slug == "roof-cross-gable-underside" {
            if slug == "roof-cross-gable-underside" {
                2.65
            } else {
                1.72
            }
        } else if slug.starts_with("roof-cross-gable-")
            && (slug.ends_with("-top") || slug.ends_with("-drainage"))
        {
            2.35
        } else if slug.starts_with("roof-cross-gable-") {
            1.75
        } else if slug.starts_with("roof-dormer-") {
            1.12
        } else if roof_focus_indices.len() > 2 {
            1.9
        } else {
            1.35
        };
        let distance = if slug.ends_with("-high-pitch") {
            roof_focus_extent.min(18.0) * 1.35 + 2.0
        } else {
            roof_focus_extent * distance_scale + 2.0
        };
        if slug.ends_with("-high-pitch") {
            roof_focus + Vec3::new(distance * 0.75, distance * 0.75, -distance)
        } else if slug.ends_with("-top") || slug.ends_with("-drainage") {
            roof_focus + Vec3::new(distance * 0.18, distance * 1.35, -distance * 0.12)
        } else if slug == "roof-dormer-gabled-exterior"
            && let Some(child_id) = plan
                .roof_assemblies
                .iter()
                .flat_map(|roof| &roof.children)
                .find(|child| {
                    child.kind == adventuresim_building_generator::RoofChildKind::GabledDormer
                })
                .map(|child| child.child)
            && let Some(wall) = plan.wall_assemblies.iter().find(|wall| {
                wall.source
                    == adventuresim_building_generator::WallSourceId::RoofChildFront {
                        roof: child_id,
                    }
            })
        {
            let outward = Vec3::new(wall.frame.outward.x, 0.0, wall.frame.outward.y);
            let tangent = Vec3::new(wall.frame.tangent.x, 0.0, wall.frame.tangent.y);
            roof_focus + outward * distance + tangent * distance * 0.34 + Vec3::Y * distance * 0.34
        } else if slug == "roof-cross-gable-exterior" {
            let cross_id = plan
                .roof_assemblies
                .iter()
                .flat_map(|roof| &roof.children)
                .find(|child| {
                    child.kind == adventuresim_building_generator::RoofChildKind::CrossGable
                        && child.facade_wall.is_some()
                })
                .map(|child| child.child);
            let outward = plan
                .wall_assemblies
                .iter()
                .find(|wall| {
                    matches!(wall.source, adventuresim_building_generator::WallSourceId::RoofChildFront { roof } if Some(roof) == cross_id)
                })
                .map(|wall| wall.frame.outward)
                .unwrap_or(-Vec2::Y);
            roof_focus + Vec3::new(outward.x, 0.0, outward.y) * distance + Vec3::Y * distance * 0.38
        } else if slug.ends_with("-underside") || slug.ends_with("-interior") {
            roof_focus + Vec3::new(distance * 0.9, distance * 0.18, -distance * 0.55)
        } else if slug == "roof-abutment-wall-cutaway" {
            roof_focus + Vec3::new(distance * 0.9, distance * 1.05, -distance * 0.8)
        } else if slug.ends_with("-cutaway") {
            roof_focus + Vec3::new(distance * 1.05, distance * 0.62, -distance * 0.90)
        } else if slug == "roof-abutment-wall-exterior" {
            roof_focus + Vec3::new(distance * 0.9, distance * 0.85, -distance * 0.8)
        } else if slug == "roof-cathedral-exterior" {
            roof_focus + Vec3::new(distance, distance * 0.68, -distance)
        } else {
            roof_focus + Vec3::new(distance, distance * 0.48, -distance)
        }
    } else {
        match view {
            ViewerView::Exterior => {
                let scale = match plan.archetype {
                    BuildingArchetype::FachwerkMerchantHouse => 1.18,
                    BuildingArchetype::ArtilleryRondelCastle => 0.92,
                    _ => 1.0,
                };
                Vec3::new(
                    radius * 0.82 * scale,
                    max_height * 0.90 * scale,
                    -radius * 1.08 * scale,
                )
            }
            ViewerView::Defenses => Vec3::new(-radius * 1.05, max_height * 1.35, radius * 1.15),
            ViewerView::Cutaway => Vec3::new(radius * 0.75, max_height * 1.8, -radius * 1.1),
            ViewerView::GateDetailExterior => {
                let focus = plan
                    .gate_defenses
                    .first()
                    .map(|defense| defense.threshold + origin)
                    .unwrap_or(Vec2::ZERO);
                Vec3::new(focus.x + 10.0, 8.0, focus.y - 15.5)
            }
            ViewerView::GateDetailInterior => {
                let focus = plan
                    .gate_defenses
                    .first()
                    .map(|defense| defense.guard_chamber.access.flight.bottom + origin)
                    .unwrap_or(Vec2::ZERO);
                // Look through the sectioned east/rear corner from above. The
                // detail renderer shortens the proof fragment of wall walk, so
                // this angle retains the whole flight without masking the chamber
                // floor, murder hole, or windlass.
                Vec3::new(focus.x + 9.5, 12.5, focus.y + 7.5)
            }
            ViewerView::TowerPortalDetail => {
                let focus = plan
                    .towers
                    .first()
                    .map(|tower| tower.centre_metres() + origin)
                    .unwrap_or(Vec2::ZERO);
                Vec3::new(focus.x + 8.0, 7.0, focus.y - 10.0)
            }
            ViewerView::CrownStraightExterior => Vec3::new(
                straight_crown_focus.0.x + 4.8,
                straight_crown_focus.1 + 3.6,
                straight_crown_focus.0.y - 6.0,
            ),
            ViewerView::CrownStraightInterior => Vec3::new(
                straight_crown_focus.0.x + 5.5,
                straight_crown_focus.1 + 4.5,
                straight_crown_focus.0.y + 6.0,
            ),
            ViewerView::CrownCornerExterior => Vec3::new(
                corner_crown_focus.0.x - 4.7,
                corner_crown_focus.1 + 3.6,
                corner_crown_focus.0.y - 4.7,
            ),
            ViewerView::CrownCornerInterior => Vec3::new(
                corner_crown_focus.0.x + 6.2,
                corner_crown_focus.1 + 4.0,
                corner_crown_focus.0.y + 6.2,
            ),
            ViewerView::CrownTowerExterior => Vec3::new(
                tower_crown_focus.0.x + 1.3,
                tower_crown_focus.1 + 3.4,
                tower_crown_focus.0.y - 6.8,
            ),
            ViewerView::CrownTowerTop => Vec3::new(
                tower_crown_focus.0.x + 1.7,
                tower_crown_focus.1 + 8.0,
                tower_crown_focus.0.y - 1.7,
            ),
            ViewerView::CrownTowerCutaway => Vec3::new(
                tower_crown_focus.0.x + 4.8,
                tower_crown_focus.1 + 4.5,
                tower_crown_focus.0.y - 4.8,
            ),
            ViewerView::ProjectedExterior | ViewerView::ProjectedSockets => {
                let close_scale = if projected_kind == ProjectedProofKind::Bartizan {
                    0.53
                } else {
                    1.0
                };
                let horizontal_distance = projected_distance * close_scale;
                let tangent_factor = if projected_kind == ProjectedProofKind::Bartizan {
                    0.25
                } else {
                    0.9
                };
                Vec3::new(
                    projected_focus.x
                        + projected_outward.x * horizontal_distance
                        + projected_tangent.x * horizontal_distance * tangent_factor,
                    projected_focus.y
                        + projected_distance
                            * if projected_kind == ProjectedProofKind::Bartizan {
                                0.95
                            } else {
                                0.32
                            },
                    projected_focus.z
                        + projected_outward.y * horizontal_distance
                        + projected_tangent.y * horizontal_distance * tangent_factor,
                )
            }
            ViewerView::ProjectedInterior if projected_kind == ProjectedProofKind::Bartizan => {
                // The grounded buttress makes the bartizan proof substantially taller
                // than the other projected works.  A close, high protected-side view
                // preserves that full load path while giving the small usable chamber
                // enough screen width for inspection.
                let horizontal_distance = projected_distance * 0.53;
                Vec3::new(
                    projected_focus.x - projected_outward.x * horizontal_distance
                        + projected_tangent.x * horizontal_distance * 0.25,
                    projected_focus.y + projected_distance * 0.95,
                    projected_focus.z - projected_outward.y * horizontal_distance
                        + projected_tangent.y * horizontal_distance * 0.25,
                )
            }
            ViewerView::ProjectedInterior => Vec3::new(
                projected_focus.x
                    - projected_outward.x * projected_distance * projected_interior_scale
                    + projected_tangent.x * projected_distance * 0.85 * projected_interior_scale,
                projected_focus.y + projected_distance * 0.3 * projected_interior_scale,
                projected_focus.z
                    - projected_outward.y * projected_distance * projected_interior_scale
                    + projected_tangent.y * projected_distance * 0.85 * projected_interior_scale,
            ),
            ViewerView::ProjectedUnderside if projected_kind == ProjectedProofKind::Bartizan => {
                let horizontal_distance = projected_distance * 0.53;
                Vec3::new(
                    projected_focus.x
                        + projected_outward.x * horizontal_distance
                        + projected_tangent.x * horizontal_distance * 0.25,
                    projected_focus.y - projected_distance * 0.95,
                    projected_focus.z
                        + projected_outward.y * horizontal_distance
                        + projected_tangent.y * horizontal_distance * 0.25,
                )
            }
            ViewerView::ProjectedUnderside => Vec3::new(
                projected_focus.x
                    + projected_outward.x * projected_distance * 1.28 * projected_underside_scale
                    + projected_tangent.x * projected_distance * 0.45 * projected_underside_scale,
                projected_focus.y - projected_distance * 0.7 * projected_underside_scale,
                projected_focus.z
                    + projected_outward.y * projected_distance * 1.28 * projected_underside_scale
                    + projected_tangent.y * projected_distance * 0.45 * projected_underside_scale,
            ),
            ViewerView::ProjectedTop if projected_kind == ProjectedProofKind::Bartizan => {
                Vec3::new(
                    projected_focus.x
                        + projected_outward.x * projected_distance * 0.27
                        + projected_tangent.x * projected_distance * 0.27,
                    projected_focus.y + projected_distance * 0.96,
                    projected_focus.z
                        + projected_outward.y * projected_distance * 0.27
                        + projected_tangent.y * projected_distance * 0.27,
                )
            }
            ViewerView::ProjectedTop => Vec3::new(
                projected_focus.x
                    + projected_outward.x * projected_distance * 0.45 * projected_top_scale
                    + projected_tangent.x * projected_distance * 0.45 * projected_top_scale,
                projected_focus.y + projected_distance * 1.60 * projected_top_scale,
                projected_focus.z
                    + projected_outward.y * projected_distance * 0.45 * projected_top_scale
                    + projected_tangent.y * projected_distance * 0.45 * projected_top_scale,
            ),
            ViewerView::ProjectedLongitudinal => Vec3::new(
                projected_focus.x
                    + projected_tangent.x * projected_distance * 1.4
                    + projected_outward.x * projected_distance * 0.4,
                projected_focus.y + projected_distance * 0.4,
                projected_focus.z
                    + projected_tangent.y * projected_distance * 1.4
                    + projected_outward.y * projected_distance * 0.4,
            ),
            ViewerView::ProjectedFlank if projected_kind == ProjectedProofKind::Bartizan => {
                let horizontal_distance = projected_distance * 0.53;
                Vec3::new(
                    projected_focus.x
                        + projected_tangent.x * horizontal_distance
                        + projected_outward.x * horizontal_distance * 0.25,
                    projected_focus.y + projected_distance * 0.95,
                    projected_focus.z
                        + projected_tangent.y * horizontal_distance
                        + projected_outward.y * horizontal_distance * 0.25,
                )
            }
            ViewerView::ProjectedFlank => Vec3::new(
                projected_focus.x
                    + projected_tangent.x * projected_distance * 0.75 * projected_flank_scale
                    + projected_outward.x * projected_distance * 0.65 * projected_flank_scale,
                projected_focus.y + projected_distance * 0.28 * projected_flank_scale,
                projected_focus.z
                    + projected_tangent.y * projected_distance * 0.75 * projected_flank_scale
                    + projected_outward.y * projected_distance * 0.65 * projected_flank_scale,
            ),
            ViewerView::OpeningRectangularSection
            | ViewerView::OpeningSegmentalSection
            | ViewerView::OpeningPointedSection
            | ViewerView::OpeningArrowLoopSection
            | ViewerView::OpeningGunLoopSection
            | ViewerView::WallTimberFrameSection
            | ViewerView::WallCivilianMasonrySection
            | ViewerView::WallCathedralButtressSection
            | ViewerView::WallRoundTowerRadialSection => Vec3::new(
                architectural_focus.x
                    + (architectural_tangent.x + architectural_outward.x * 0.55)
                        * architectural_distance,
                architectural_focus.y + architectural_distance * 0.22,
                architectural_focus.z
                    + (architectural_tangent.y + architectural_outward.y * 0.55)
                        * architectural_distance,
            ),
            ViewerView::OpeningRectangularInterior
            | ViewerView::OpeningSegmentalInterior
            | ViewerView::OpeningPointedInterior
            | ViewerView::OpeningArrowLoopInterior
            | ViewerView::OpeningGunLoopInterior => Vec3::new(
                architectural_focus.x - architectural_outward.x * architectural_distance
                    + architectural_tangent.x * architectural_distance * 0.30,
                architectural_focus.y + architectural_distance * 0.18,
                architectural_focus.z - architectural_outward.y * architectural_distance
                    + architectural_tangent.y * architectural_distance * 0.30,
            ),
            ViewerView::OpeningRectangularExterior
            | ViewerView::OpeningSegmentalExterior
            | ViewerView::OpeningPointedExterior
            | ViewerView::OpeningArrowLoopExterior
            | ViewerView::OpeningGunLoopExterior => Vec3::new(
                architectural_focus.x
                    + architectural_outward.x * architectural_distance
                    + architectural_tangent.x * architectural_distance * 0.30,
                architectural_focus.y + architectural_distance * 0.18,
                architectural_focus.z
                    + architectural_outward.y * architectural_distance
                    + architectural_tangent.y * architectural_distance * 0.30,
            ),
            _ => Vec3::new(radius, max_height * 0.95, -radius * 1.3),
        }
    };
    let target = if let Some((_, focus)) = artillery_camera {
        focus
    } else if let Some((_, focus)) = timber_camera {
        focus
    } else if let Some((_, focus)) = church_camera {
        focus
    } else if roof_proof.is_some() {
        roof_focus
    } else {
        match view {
            ViewerView::Exterior => Vec3::new(0.0, max_height * 0.42, 0.0),
            ViewerView::GateDetailExterior => plan
                .gate_defenses
                .first()
                .map(|defense| {
                    let focus = defense.threshold + origin;
                    Vec3::new(focus.x, 3.4, focus.y)
                })
                .unwrap_or(target),
            ViewerView::GateDetailInterior => plan
                .gate_defenses
                .first()
                .map(|defense| {
                    let route = defense
                        .guard_chamber
                        .access
                        .flight
                        .top
                        .lerp(defense.guard_chamber.access.flight.bottom, 0.55);
                    let focus = route.lerp(defense.guard_chamber.centre, 0.45) + origin;
                    Vec3::new(focus.x, 4.7, focus.y)
                })
                .unwrap_or(target),
            ViewerView::TowerPortalDetail => plan
                .towers
                .first()
                .map(|tower| {
                    let focus = tower.centre_metres() + origin;
                    Vec3::new(focus.x, tower.wall_height_metres * 0.48, focus.y)
                })
                .unwrap_or(target),
            ViewerView::CrownStraightExterior | ViewerView::CrownStraightInterior => Vec3::new(
                straight_crown_focus.0.x,
                straight_crown_focus.1 + 0.9,
                straight_crown_focus.0.y,
            ),
            ViewerView::CrownCornerExterior | ViewerView::CrownCornerInterior => Vec3::new(
                corner_crown_focus.0.x,
                corner_crown_focus.1 + 0.9,
                corner_crown_focus.0.y,
            ),
            ViewerView::CrownTowerExterior
            | ViewerView::CrownTowerTop
            | ViewerView::CrownTowerCutaway => Vec3::new(
                tower_crown_focus.0.x,
                tower_crown_focus.1 + 0.8,
                tower_crown_focus.0.y,
            ),
            ViewerView::ProjectedExterior | ViewerView::ProjectedInterior
                if projected_kind == ProjectedProofKind::Bartizan =>
            {
                projected_focus + Vec3::Y * 0.3
            }
            ViewerView::ProjectedUnderside if projected_kind == ProjectedProofKind::Bartizan => {
                projected_focus + Vec3::Y * 0.8
            }
            ViewerView::ProjectedTop if projected_kind == ProjectedProofKind::Bartizan => {
                projected_focus + Vec3::Y * 0.4
            }
            ViewerView::ProjectedFlank if projected_kind == ProjectedProofKind::Bartizan => {
                projected_focus + Vec3::Y * 0.3
            }
            ViewerView::ProjectedExterior
            | ViewerView::ProjectedInterior
            | ViewerView::ProjectedUnderside
            | ViewerView::ProjectedTop
            | ViewerView::ProjectedLongitudinal
            | ViewerView::ProjectedSockets
            | ViewerView::ProjectedFlank => projected_focus,
            _ if architectural_proof => architectural_focus,
            _ => target,
        }
    };
    let sun_position = if projected_proof {
        let (outward_scale, tangent_scale) = if matches!(
            view,
            ViewerView::ProjectedLongitudinal | ViewerView::ProjectedTop
        ) {
            (34.0, 18.0)
        } else {
            (18.0, 34.0)
        };
        Vec3::new(
            projected_outward.x * outward_scale + projected_tangent.x * tangent_scale,
            45.0,
            projected_outward.y * outward_scale + projected_tangent.y * tangent_scale,
        )
    } else if roof_proof.is_some_and(|proof| roof_proof_slug(proof) == "roof-cross-gable-exterior")
    {
        Vec3::new(-28.0, 38.0, -20.0)
    } else {
        match view {
            ViewerView::GateDetailInterior => {
                // Bailey-side light is the deterministic section-view fill: it
                // enters through the deliberately removed rear wall and reveals
                // the floor, stair, windlass, and closures without emissive parts.
                Vec3::new(28.0, 50.0, 34.0)
            }
            ViewerView::GateDetailExterior => Vec3::new(28.0, 38.0, -34.0),
            ViewerView::TowerPortalDetail => Vec3::new(28.0, 50.0, -34.0),
            ViewerView::CrownStraightInterior => Vec3::new(-28.0, 45.0, 34.0),
            ViewerView::CrownCornerInterior => Vec3::new(28.0, 45.0, 34.0),
            // The defensive overview camera occupies the opposite quadrant from
            // the ordinary exterior camera. Keep the key oblique but move it to
            // the visible side so wall thickness and tower curvature remain read.
            ViewerView::Defenses => Vec3::new(-34.0, 38.0, 20.0),
            ViewerView::TimberFrameFacade => Vec3::new(34.0, 40.0, -8.0),
            _ => Vec3::new(28.0, 38.0, -34.0),
        }
    };
    let camera_up = if view == ViewerView::ProjectedUnderside
        && projected_kind == ProjectedProofKind::Bartizan
    {
        // A restrained roll keeps the full-height bonded buttress and the
        // underside work simultaneously measurable in the portrait-like proof.
        // It changes presentation only; all focus bounds still derive from the
        // exact resolved assembly IDs.
        (Vec3::Y + Vec3::new(projected_tangent.x, 0.0, projected_tangent.y) * 0.40).normalize()
    } else {
        Vec3::Y
    };
    if scene_setup != SceneSetup::EditorBuilding {
        world.spawn((
            Camera3d::default(),
            Transform::from_translation(camera_position).looking_at(target, camera_up),
        ));
        {
            let mut capture = world.resource_mut::<CaptureState>();
            capture.manifest.camera_position = camera_position.to_array();
            capture.manifest.camera_target = target.to_array();
        }
        world.spawn((
            DirectionalLight {
                illuminance: if projected_proof {
                    20_000.0
                } else if crown_proof || roof_proof.is_some() || timber_proof_suffix(view).is_some()
                {
                    28_000.0
                } else {
                    24_000.0
                },
                shadow_maps_enabled: true,
                ..default()
            },
            // An oblique south-eastern key separates the gate front, return walls,
            // tower curvature, and projecting crown. `looking_at` keeps the light
            // direction legible instead of relying on opaque Euler rotations.
            Transform::from_translation(sun_position).looking_at(Vec3::ZERO, Vec3::Y),
        ));
        if roof_proof.is_some_and(|proof| roof_proof_slug(proof).ends_with("-interior")) {
            // Section proofs expose the unlit underside of a physically opaque
            // roof. A restrained, deterministic attic fill keeps rafters and the
            // surviving weather face readable without flattening the exterior key.
            world.spawn((
                PointLight {
                    intensity: 75_000.0,
                    range: (roof_focus_extent * 2.5).max(12.0),
                    shadow_maps_enabled: false,
                    color: Color::srgb(0.78, 0.84, 0.94),
                    ..default()
                },
                Transform::from_translation(roof_focus - Vec3::Y * roof_focus_extent * 0.35),
            ));
        }
        if church_section_proof(view)
            && let Some((camera, focus)) = church_camera
        {
            // A restrained camera-side working fill makes the exposed vault,
            // springing, and service-route faces readable without replacing the
            // oblique shadowed daylight used by the whole-building regressions.
            world.spawn((
                PointLight {
                    intensity: 85_000.0,
                    range: camera.distance(focus).clamp(18.0, 36.0),
                    shadow_maps_enabled: false,
                    color: Color::srgb(0.78, 0.84, 0.94),
                    ..default()
                },
                Transform::from_translation(camera.lerp(focus, 0.32)),
            ));
        }
        if view == ViewerView::ArtilleryGateInterior
            && let Some((camera, focus)) = artillery_camera
        {
            world.spawn((
                PointLight {
                    intensity: 320_000.0,
                    range: 28.0,
                    shadow_maps_enabled: false,
                    color: Color::srgb(0.78, 0.84, 0.94),
                    ..default()
                },
                Transform::from_translation(camera.lerp(focus, 0.38)),
            ));
        }
        if view == ViewerView::ArtilleryRondelCasemate
            && let Some((camera, focus)) = artillery_camera
        {
            // Working daylight inside the opened casemate: this remains a lit,
            // shadowed material proof, but the camera-side fill prevents the two
            // surviving gun recesses and smoke throats from collapsing to black.
            world.spawn((
                PointLight {
                    intensity: 180_000.0,
                    range: 22.0,
                    shadow_maps_enabled: false,
                    color: Color::srgb(0.78, 0.84, 0.94),
                    ..default()
                },
                Transform::from_translation(camera.lerp(focus, 0.30)),
            ));
        }
        if roof_proof.is_some_and(|proof| roof_proof_slug(proof) == "roof-cross-gable-exterior") {
            // The facade-derived Zwerchhaus faces west in the curated fixture and
            // is consequently on the key-light shadow side.  A restrained cool
            // proof fill reveals its jambs, eave split, and apron without erasing
            // the directional roof modeling.
            world.spawn((
                PointLight {
                    intensity: 95_000.0,
                    range: 18.0,
                    shadow_maps_enabled: false,
                    color: Color::srgb(0.78, 0.84, 0.94),
                    ..default()
                },
                Transform::from_translation(roof_focus + Vec3::new(-5.0, 4.0, 2.0)),
            ));
        }
        if roof_proof.is_some_and(|proof| {
            matches!(
                roof_proof_slug(proof),
                "roof-abutment-tower-exterior" | "roof-abutment-tower-cutaway"
            )
        }) {
            world.spawn((
                PointLight {
                    intensity: 70_000.0,
                    range: 20.0,
                    shadow_maps_enabled: false,
                    color: Color::srgb(0.80, 0.85, 0.92),
                    ..default()
                },
                Transform::from_translation(roof_focus + Vec3::new(5.0, 3.0, -5.0)),
            ));
        }
        world.insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.72, 0.78, 0.88),
            brightness: if view == ViewerView::ProjectedSockets {
                300.0
            } else if view == ViewerView::ProjectedInterior {
                420.0
            } else if view == ViewerView::ProjectedUnderside {
                380.0
            } else if roof_proof.is_some_and(|proof| roof_proof_slug(proof).ends_with("-interior"))
            {
                400.0
            } else if roof_proof
                .is_some_and(|proof| roof_proof_slug(proof) == "roof-cross-gable-exterior")
            {
                320.0
            } else if roof_proof.is_some() {
                240.0
            } else if crown_proof || projected_proof {
                340.0
            } else if timber_proof_suffix(view).is_some() {
                220.0
            } else {
                380.0
            },
            affects_lightmapped_meshes: true,
        });
    }
    world.insert_resource(palette);
    record_mesh_audit(world);
}
