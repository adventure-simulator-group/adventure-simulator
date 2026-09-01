fn church_focus_item_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    let Some(church) = &plan.church else {
        return Vec::new();
    };
    let church_wall_owners = plan
        .wall_assemblies
        .iter()
        .filter(|wall| {
            matches!(
                wall.source,
                adventuresim_building_generator::WallSourceId::ChurchExterior { .. }
                    | adventuresim_building_generator::WallSourceId::ChurchArcade { .. }
                    | adventuresim_building_generator::WallSourceId::ChurchCrossing { .. }
                    | adventuresim_building_generator::WallSourceId::ChurchApse { .. }
                    | adventuresim_building_generator::WallSourceId::ChurchTowerFace { .. }
                    | adventuresim_building_generator::WallSourceId::SquareTowerFace { .. }
            )
        })
        .map(|wall| wall.owner)
        .collect::<std::collections::HashSet<_>>();
    let class_matches = |solid: &adventuresim_building_generator::ResolvedSolid| {
        church_wall_owners.contains(&solid.owner)
            || matches!(
                solid.role,
                SolidRole::ChurchFloor
                    | SolidRole::ChurchPier
                    | SolidRole::ChurchArcade
                    | SolidRole::ChurchVaultShell
                    | SolidRole::ChurchVaultThrust
                    | SolidRole::ChurchCrossingArch
                    | SolidRole::ChurchBellFloor
                    | SolidRole::ChurchBellFrame
                    | SolidRole::ChurchBell
                    | SolidRole::ChurchGuard
                    | SolidRole::ChurchStairNewel
                    | SolidRole::ChurchStairTread
                    | SolidRole::ChurchServiceLadder
                    | SolidRole::Landing
                    | SolidRole::WallButtress
                    | SolidRole::FrameMember
            )
    };
    plan.resolved_geometry
        .solids
        .iter()
        .filter(|solid| {
            if matches!(
                view,
                ViewerView::ChurchDrainage | ViewerView::ChurchTowerRoofDrain
            ) {
                let drainage_role = matches!(
                    solid.role,
                    SolidRole::RoofGutter | SolidRole::RoofFlashing | SolidRole::RoofEdgeTreatment
                );
                return drainage_role
                    && (!matches!(view, ViewerView::ChurchTowerRoofDrain)
                        || solid.centre.x <= church.tower.centre.x + 4.5);
            }
            if !class_matches(solid) {
                return false;
            }
            match view {
                ViewerView::ChurchBayExterior
                | ViewerView::ChurchBayInterior
                | ViewerView::ChurchBaySection
                | ViewerView::ChurchBayLoad
                | ViewerView::ChurchBayVault => {
                    let in_bay = (solid.centre.x - church.nave_axes_metres[1]).abs() <= 2.8;
                    in_bay
                        && match view {
                            ViewerView::ChurchBaySection => matches!(
                                solid.role,
                                SolidRole::ChurchPier
                                    | SolidRole::ChurchArcade
                                    | SolidRole::ChurchFloor
                            ),
                            ViewerView::ChurchBayLoad => matches!(
                                solid.role,
                                SolidRole::ChurchPier
                                    | SolidRole::WallButtress
                                    | SolidRole::ChurchVaultThrust
                                    | SolidRole::ChurchVaultShell
                            ),
                            ViewerView::ChurchBayVault => matches!(
                                solid.role,
                                SolidRole::ChurchPier
                                    | SolidRole::ChurchVaultShell
                                    | SolidRole::ChurchVaultThrust
                            ),
                            ViewerView::ChurchBayInterior => matches!(
                                solid.role,
                                SolidRole::ChurchFloor
                                    | SolidRole::ChurchPier
                                    | SolidRole::ChurchArcade
                            ),
                            _ => true,
                        }
                }
                ViewerView::ChurchCrossingInterior
                | ViewerView::ChurchCrossingExterior
                | ViewerView::ChurchCrossingTop
                | ViewerView::ChurchCrossingCutLoad => {
                    (solid.centre.x - church.crossing_axis_metres).abs() <= 3.0
                        && (!matches!(view, ViewerView::ChurchCrossingCutLoad)
                            || matches!(
                                solid.role,
                                SolidRole::ChurchCrossingArch
                                    | SolidRole::ChurchPier
                                    | SolidRole::ChurchVaultShell
                                    | SolidRole::ChurchVaultThrust
                                    | SolidRole::WallButtress
                            ))
                }
                ViewerView::ChurchChoirEast
                | ViewerView::ChurchChoirInterior
                | ViewerView::ChurchChoirTop
                | ViewerView::ChurchChoirRadialSection => {
                    solid.centre.x >= church.crossing_axis_metres + 2.0
                        && (!matches!(
                            view,
                            ViewerView::ChurchChoirInterior | ViewerView::ChurchChoirRadialSection
                        ) || matches!(
                            solid.role,
                            SolidRole::WallHost
                                | SolidRole::OpeningJamb
                                | SolidRole::OpeningSill
                                | SolidRole::OpeningHead
                                | SolidRole::OpeningSpandrel
                                | SolidRole::ChurchFloor
                                | SolidRole::ChurchPier
                                | SolidRole::ChurchArcade
                                | SolidRole::ChurchVaultShell
                                | SolidRole::ChurchVaultThrust
                                | SolidRole::WallButtress
                        ))
                }
                ViewerView::ChurchTowerPortal
                | ViewerView::ChurchTowerJunction
                | ViewerView::ChurchTowerStair
                | ViewerView::ChurchTowerBellUnderside
                | ViewerView::ChurchTowerFrame
                | ViewerView::ChurchTowerLouvredExterior => {
                    // Include the bonded first nave-bay return as part of the
                    // westwork proof, rather than treating the tall tower as
                    // an isolated freestanding object.
                    let in_westwork = solid.centre.x <= church.tower.centre.x + 5.5;
                    in_westwork
                        && match view {
                            ViewerView::ChurchTowerStair => matches!(
                                solid.role,
                                SolidRole::ChurchStairNewel
                                    | SolidRole::ChurchStairTread
                                    | SolidRole::ChurchFloor
                                    | SolidRole::ChurchGuard
                                    | SolidRole::Landing
                            ),
                            ViewerView::ChurchTowerBellUnderside => matches!(
                                solid.role,
                                SolidRole::ChurchBellFloor
                                    | SolidRole::ChurchBell
                                    | SolidRole::ChurchFloor
                            ),
                            ViewerView::ChurchTowerFrame => matches!(
                                solid.role,
                                SolidRole::ChurchBellFrame
                                    | SolidRole::ChurchBell
                                    | SolidRole::ChurchServiceLadder
                                    | SolidRole::ChurchBellFloor
                            ),
                            ViewerView::ChurchTowerJunction => {
                                solid.centre.y <= 4.25
                                    && (matches!(
                                        solid.role,
                                        SolidRole::ChurchPier
                                            | SolidRole::ChurchArcade
                                            | SolidRole::ChurchFloor
                                            | SolidRole::ChurchVaultThrust
                                            | SolidRole::ChurchStairTread
                                            | SolidRole::ChurchStairNewel
                                            | SolidRole::Landing
                                    ) || church_wall_owners.contains(&solid.owner))
                            }
                            ViewerView::ChurchTowerPortal => {
                                church_wall_owners.contains(&solid.owner) && solid.centre.y <= 5.5
                            }
                            ViewerView::ChurchTowerLouvredExterior => {
                                church_wall_owners.contains(&solid.owner) && solid.centre.y >= 13.0
                            }
                            _ => true,
                        }
                }
                ViewerView::ChurchSupportDag => {
                    (solid.centre.x - church.nave_axes_metres[1]).abs() <= 2.8
                        && matches!(
                            solid.role,
                            SolidRole::ChurchPier
                                | SolidRole::WallButtress
                                | SolidRole::ChurchVaultThrust
                                | SolidRole::ChurchVaultShell
                                | SolidRole::ChurchArcade
                        )
                }
                _ => true,
            }
        })
        .map(|solid| solid.id.0)
        .collect()
}

fn focused_crown_item_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    let focus = match view {
        ViewerView::CrownStraightExterior | ViewerView::CrownStraightInterior => {
            plan.crowns.iter().find_map(|crown| match crown.path {
                CrownPath::Straight { start, end, .. } => {
                    Some((vec![crown.owner], (start + end) * 0.5))
                }
                CrownPath::Round { .. } => None,
            })
        }
        ViewerView::CrownCornerExterior | ViewerView::CrownCornerInterior => plan
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
            .map(|(crown, junction)| (vec![crown.owner, junction.other_owner], junction.position)),
        ViewerView::CrownTowerExterior
        | ViewerView::CrownTowerTop
        | ViewerView::CrownTowerCutaway => {
            let preferred = plan
                .gate_defenses
                .first()
                .and_then(|gate| gate.firing_positions.first())
                .map(|position| position.tower_index);
            plan.crowns.iter().find_map(|crown| match crown.path {
                CrownPath::Round {
                    tower_index,
                    centre,
                    ..
                } if preferred.is_none_or(|value| value == tower_index) => {
                    Some((vec![crown.owner], centre))
                }
                _ => None,
            })
        }
        _ => None,
    };
    let Some((owners, focus)) = focus else {
        return Vec::new();
    };
    owners
        .iter()
        .flat_map(|owner| {
            [
                SolidRole::Breastwork,
                SolidRole::Merlon,
                SolidRole::Coping,
                SolidRole::EdgeGuard,
            ]
            .into_iter()
            .filter_map(|role| {
                plan.resolved_geometry
                    .solids
                    .iter()
                    .filter(|solid| solid.owner == *owner && solid.role == role)
                    .min_by(|a, b| {
                        Vec2::new(a.centre.x, a.centre.z)
                            .distance_squared(focus)
                            .total_cmp(&Vec2::new(b.centre.x, b.centre.z).distance_squared(focus))
                    })
                    .map(|solid| solid.id.0)
            })
        })
        .collect()
}

fn projected_view(view: ViewerView) -> bool {
    matches!(
        view,
        ViewerView::ProjectedExterior
            | ViewerView::ProjectedInterior
            | ViewerView::ProjectedUnderside
            | ViewerView::ProjectedTop
            | ViewerView::ProjectedLongitudinal
            | ViewerView::ProjectedSockets
            | ViewerView::ProjectedFlank
    )
}

fn opening_proof_profile(view: ViewerView) -> Option<&'static str> {
    match view {
        ViewerView::OpeningRectangularExterior
        | ViewerView::OpeningRectangularInterior
        | ViewerView::OpeningRectangularSection => Some("rectangular"),
        ViewerView::OpeningSegmentalExterior
        | ViewerView::OpeningSegmentalInterior
        | ViewerView::OpeningSegmentalSection => Some("segmental"),
        ViewerView::OpeningPointedExterior
        | ViewerView::OpeningPointedInterior
        | ViewerView::OpeningPointedSection => Some("pointed_two_centred"),
        ViewerView::OpeningArrowLoopExterior
        | ViewerView::OpeningArrowLoopInterior
        | ViewerView::OpeningArrowLoopSection => Some("arrow_loop"),
        ViewerView::OpeningGunLoopExterior
        | ViewerView::OpeningGunLoopInterior
        | ViewerView::OpeningGunLoopSection => Some("gun_loop"),
        _ => None,
    }
}

fn wall_section_kind(view: ViewerView) -> Option<&'static str> {
    match view {
        ViewerView::WallTimberFrameSection => Some("timber_frame"),
        ViewerView::WallCivilianMasonrySection => Some("civilian_masonry"),
        ViewerView::WallCathedralButtressSection => Some("cathedral_buttress"),
        ViewerView::WallRoundTowerRadialSection => Some("round_tower_radial"),
        _ => None,
    }
}

fn architectural_proof(view: ViewerView) -> bool {
    opening_proof_profile(view).is_some() || wall_section_kind(view).is_some()
}

fn section_proof(view: ViewerView) -> bool {
    matches!(
        view,
        ViewerView::OpeningRectangularSection
            | ViewerView::OpeningSegmentalSection
            | ViewerView::OpeningPointedSection
            | ViewerView::OpeningArrowLoopSection
            | ViewerView::OpeningGunLoopSection
            | ViewerView::WallTimberFrameSection
            | ViewerView::WallCivilianMasonrySection
            | ViewerView::WallCathedralButtressSection
            | ViewerView::WallRoundTowerRadialSection
    )
}

fn opening_profile_slug(profile: adventuresim_building_generator::OpeningProfile) -> &'static str {
    use adventuresim_building_generator::OpeningProfile;
    match profile {
        OpeningProfile::Rectangular { .. } => "rectangular",
        OpeningProfile::Segmental { .. } => "segmental",
        OpeningProfile::PointedTwoCentred { .. } => "pointed_two_centred",
        OpeningProfile::ArrowLoop { .. } => "arrow_loop",
        OpeningProfile::GunLoop { .. } => "gun_loop",
    }
}

fn focused_opening(
    plan: &BuildingPlan,
    view: ViewerView,
) -> Option<&adventuresim_building_generator::OpeningAssembly> {
    let profile = opening_proof_profile(view)?;
    plan.opening_assemblies
        .iter()
        .filter(|opening| opening_profile_slug(opening.profile) == profile)
        .min_by_key(|opening| {
            (
                usize::from(opening.frame.outside_room.is_some()),
                opening.host_wall.0,
            )
        })
}

fn focused_wall(
    plan: &BuildingPlan,
    view: ViewerView,
) -> Option<&adventuresim_building_generator::WallAssembly> {
    use adventuresim_building_generator::WallMaterialClass;
    let kind = wall_section_kind(view)?;
    if kind == "round_tower_radial" {
        return plan.wall_assemblies.iter().find(|wall| {
            matches!(
                wall.source,
                adventuresim_building_generator::WallSourceId::RoundTower { tower_index: 0 }
            )
        });
    }
    plan.wall_assemblies
        .iter()
        .filter(|wall| {
            wall.opening_ids.is_empty()
                && wall.frame.outside_room.is_none()
                && wall
                    .host_solids
                    .iter()
                    .filter(|id| {
                        plan.resolved_geometry
                            .solids
                            .iter()
                            .find(|solid| solid.id == **id)
                            .is_some_and(|solid| solid.role == SolidRole::WallHost)
                    })
                    .count()
                    >= 2
        })
        .find(|wall| match kind {
            "timber_frame" => wall.material == WallMaterialClass::TimberInfill,
            "civilian_masonry" => wall.material == WallMaterialClass::CivilianMasonry,
            "cathedral_buttress" => wall.material == WallMaterialClass::CathedralMasonry,
            _ => false,
        })
}

fn architectural_section_removed_item_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    if artillery_section_proof(view) {
        return artillery_section_removed_item_ids(plan, view);
    }
    if timber_section_proof(view) {
        return timber_section_removed_item_ids(plan, view);
    }
    if church_section_proof(view) {
        let Some(church) = &plan.church else {
            return Vec::new();
        };
        let transverse = matches!(
            view,
            ViewerView::ChurchWholeTransverseCut | ViewerView::ChurchCrossingCutLoad
        );
        let radial_cut = (view == ViewerView::ChurchChoirRadialSection).then(|| {
            church
                .choir
                .bay_axes_metres
                .last()
                .copied()
                .unwrap_or(church.crossing_axis_metres)
                + 5.0
        });
        return plan
            .resolved_geometry
            .solids
            .iter()
            .filter(|solid| {
                let beyond_cut = radial_cut.map_or_else(
                    || {
                        if transverse {
                            solid.centre.x > church.crossing_axis_metres + 0.05
                        } else {
                            solid.centre.z < church.tower.centre.y - 0.05
                        }
                    },
                    |cut| solid.centre.x > cut + 0.05,
                );
                beyond_cut
                    && !matches!(
                        solid.role,
                        SolidRole::ChurchFloor
                            | SolidRole::ChurchBellFloor
                            | SolidRole::Landing
                            | SolidRole::ChurchGuard
                            | SolidRole::ChurchBellFrame
                            | SolidRole::ChurchBell
                            | SolidRole::FrameMember
                    )
            })
            .map(|solid| solid.id.0)
            .collect();
    }
    if let Some(opening) = focused_opening(plan, view) {
        let mut removed = vec![opening.jamb_solids[1].0];
        if let Some(reveal) = opening.reveal_surfaces.get(1) {
            removed.push(reveal.0);
        }
        return removed;
    }
    let Some(wall) = focused_wall(plan, view) else {
        return Vec::new();
    };
    if wall.radial_frame.is_some() {
        return Vec::new();
    }
    plan.resolved_geometry
        .solids
        .iter()
        .filter(|solid| solid.owner == wall.owner)
        .filter(|solid| {
            let plan_centre = Vec2::new(solid.centre.x, solid.centre.z);
            (plan_centre - wall.frame.origin).dot(wall.frame.tangent) > 0.01
        })
        .map(|solid| solid.id.0)
        .collect()
}

fn architectural_focus_owner(plan: &BuildingPlan, view: ViewerView) -> Option<u32> {
    focused_opening(plan, view)
        .map(|opening| opening.owner.0)
        .or_else(|| focused_wall(plan, view).map(|wall| wall.owner.0))
}

fn architectural_focus_item_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    let Some(owner) = architectural_focus_owner(plan, view) else {
        return Vec::new();
    };
    let mut ids = plan
        .resolved_geometry
        .solids
        .iter()
        .filter(|solid| solid.owner.0 == owner)
        .map(|solid| solid.id.0)
        .collect::<Vec<_>>();
    ids.extend(
        plan.resolved_geometry
            .surfaces
            .iter()
            .filter(|surface| surface.owner.0 == owner)
            .map(|surface| surface.id.0),
    );
    ids
}

fn architectural_focus_void_ids(plan: &BuildingPlan, view: ViewerView) -> Vec<u64> {
    focused_opening(plan, view)
        .map(|opening| vec![opening.void_id.0])
        .unwrap_or_default()
}

fn projected_kind_matches(
    defense: &adventuresim_building_generator::ProjectedDefenseAssembly,
    kind: ProjectedProofKind,
) -> bool {
    use adventuresim_building_generator::ProjectedDefenseKind;
    matches!(
        (defense.kind, kind),
        (
            ProjectedDefenseKind::Machicolation,
            ProjectedProofKind::Machicolation
        ) | (ProjectedDefenseKind::Breteche, ProjectedProofKind::Breteche)
            | (ProjectedDefenseKind::Hoarding, ProjectedProofKind::Hoarding)
            | (ProjectedDefenseKind::Bartizan, ProjectedProofKind::Bartizan)
    )
}

const fn projected_kind_slug(kind: ProjectedProofKind) -> &'static str {
    match kind {
        ProjectedProofKind::Machicolation => "machicolation",
        ProjectedProofKind::Breteche => "breteche",
        ProjectedProofKind::Hoarding => "hoarding",
        ProjectedProofKind::Bartizan => "bartizan",
    }
}

const fn projected_deployment_slug(deployment: ProjectedDefenseDeployment) -> &'static str {
    match deployment {
        ProjectedDefenseDeployment::Permanent => "permanent",
        ProjectedDefenseDeployment::SocketsOnly => "sockets_only",
        ProjectedDefenseDeployment::Deployed => "deployed",
    }
}

const fn projected_target_slug(target: ProjectedDefenseTarget) -> &'static str {
    match target {
        ProjectedDefenseTarget::GateApproach => "gate_approach",
        ProjectedDefenseTarget::ThreatenedWallFoot => "threatened_wall_foot",
        ProjectedDefenseTarget::ThreatenedCorner => "threatened_corner",
        ProjectedDefenseTarget::CampaignSiegeFront => "campaign_siege_front",
    }
}

fn focused_projected_defense(
    plan: &BuildingPlan,
    view: ViewerView,
    kind: ProjectedProofKind,
) -> Option<&adventuresim_building_generator::ProjectedDefenseAssembly> {
    use adventuresim_building_generator::ProjectedDefenseDeployment;
    plan.projected_defenses.iter().find(|defense| {
        projected_kind_matches(defense, kind)
            && if view == ViewerView::ProjectedSockets {
                defense.deployment == ProjectedDefenseDeployment::SocketsOnly
            } else if kind == ProjectedProofKind::Hoarding {
                defense.deployment == ProjectedDefenseDeployment::Deployed
            } else {
                true
            }
    })
}

fn focused_projected_item_ids(
    plan: &BuildingPlan,
    view: ViewerView,
    kind: ProjectedProofKind,
) -> Vec<u64> {
    let Some(defense) = focused_projected_defense(plan, view, kind) else {
        return Vec::new();
    };
    plan.resolved_geometry
        .solids
        .iter()
        .filter(|solid| solid.owner == defense.owner || solid.owner == defense.host_owner)
        .map(|solid| solid.id.0)
        .collect()
}
