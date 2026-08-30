fn audit_wall_opening_assemblies(plan: &BuildingPlan, issues: &mut Vec<AuditIssue>) {
    use crate::{
        ClosureKind, OpeningHeadKind, OpeningProfile, OpeningUse, ResolvedItemId,
        WallMaterialClass, WallSourceId,
    };
    let expected_walls = if let Some(church) = &plan.church {
        usize::from(church.program.nave_bays) * 2
            + usize::from(church.program.nave_bays) * 2
            + usize::from(church.program.choir_bays) * 2
            + 8
            + usize::from(church.program.apse_sides)
            + 8
            + 4
    } else {
        plan.storeys
            .iter()
            .map(|storey| storey.walls.len())
            .sum::<usize>()
            + if matches!(
                plan.archetype,
                BuildingArchetype::CastleGatehouse
                    | BuildingArchetype::CourtyardCastle
                    | BuildingArchetype::WalledKeep
                    | BuildingArchetype::ArtilleryRondelCastle
            ) {
                plan.towers.len()
            } else {
                0
            }
            + plan
                .square_towers
                .iter()
                .filter(|tower| tower.bell_openings)
                .count()
                * 8
            + if plan.archetype == BuildingArchetype::Cathedral {
                2
            } else {
                0
            }
            + plan.roof_dormers.len()
            + plan
                .artillery_castle
                .as_ref()
                .map_or(0, |castle| castle.stations.len())
    };
    if plan.wall_assemblies.len() != expected_walls {
        issues.push(issue(
            "legacy_wall_not_migrated",
            format!(
                "resolved {} of {expected_walls} storey walls",
                plan.wall_assemblies.len()
            ),
        ));
    }
    let solids = plan
        .resolved_geometry
        .solids
        .iter()
        .map(|solid| (solid.id, solid))
        .collect::<std::collections::HashMap<_, _>>();
    let surfaces = plan
        .resolved_geometry
        .surfaces
        .iter()
        .map(|surface| (surface.id, surface))
        .collect::<std::collections::HashMap<_, _>>();
    let nodes = plan
        .resolved_geometry
        .structural_nodes
        .iter()
        .map(|node| (node.id, node))
        .collect::<std::collections::HashMap<_, _>>();
    let mut wall_ids = std::collections::HashSet::new();
    let mut wall_sources = std::collections::HashSet::new();
    for wall in &plan.wall_assemblies {
        if !wall_ids.insert(wall.id)
            || !wall_sources.insert(wall.source)
            || (wall.frame.tangent.length() - 1.0).abs() > 0.001
            || (wall.frame.outward.length() - 1.0).abs() > 0.001
            || wall.frame.tangent.dot(wall.frame.outward).abs() > 0.001
            || (!matches!(wall.source, WallSourceId::ChurchApse { .. })
                && !matches!(
                    (wall.frame.tangent, wall.frame.outward),
                    (
                        Vec2 {
                            x: -1.0 | 0.0 | 1.0,
                            y: -1.0 | 0.0 | 1.0
                        },
                        Vec2 {
                            x: -1.0 | 0.0 | 1.0,
                            y: -1.0 | 0.0 | 1.0
                        }
                    )
                ))
        {
            issues.push(issue(
                "invalid_wall_authority",
                format!(
                    "wall {} has duplicate source/ID or a non-cardinal local frame",
                    wall.id.0
                ),
            ));
        }
        let valid_thickness = match wall.material {
            WallMaterialClass::TimberInfill => (0.18..=0.24).contains(&wall.thickness_metres),
            WallMaterialClass::CivilianMasonry => (0.40..=0.70).contains(&wall.thickness_metres),
            WallMaterialClass::CathedralMasonry => (0.75..=1.10).contains(&wall.thickness_metres),
            WallMaterialClass::FortifiedMasonry => wall.thickness_metres >= 1.20,
            WallMaterialClass::InternalTimber => (0.12..=0.18).contains(&wall.thickness_metres),
            WallMaterialClass::InternalMasonry => (0.20..=0.35).contains(&wall.thickness_metres),
        };
        if !valid_thickness {
            issues.push(issue(
                "wall_profile_thickness",
                format!(
                    "wall {} violates its material/profile thickness table",
                    wall.id.0
                ),
            ));
        }
        let requires_semantic_frame = matches!(
            plan.archetype,
            BuildingArchetype::TownHouse
                | BuildingArchetype::HallHouse
                | BuildingArchetype::FachwerkCottage
                | BuildingArchetype::FachwerkMerchantHouse
                | BuildingArchetype::RenaissanceTownHall
        );
        if requires_semantic_frame
            && wall.material == WallMaterialClass::TimberInfill
            && !plan.timber_frame.as_ref().is_some_and(|frame| {
                frame.bays.iter().any(|bay| {
                    bay.wall == Some(wall.id)
                        && bay.member_ids.len() >= 4
                        && bay.member_ids.iter().all(|id| {
                            frame.members.iter().any(|member| {
                                member.id == *id
                                    && member.structural
                                    && member.role != crate::TimberMemberRole::Ornament
                            })
                        })
                })
            })
        {
            issues.push(issue(
                "missing_authoritative_timber_frame",
                format!(
                    "timber wall {} suppresses its source without an opening-first semantic load bay",
                    wall.id.0
                ),
            ));
        }
        match wall.source {
            crate::WallSourceId::RoundTower { tower_index } => {
                let tower = plan.towers.get(tower_index);
                let radial = wall.radial_frame;
                let shell = wall.host_solids.first().and_then(|id| solids.get(id));
                let valid = tower.is_some_and(|tower| {
                    radial.is_some_and(|radial| {
                        radial.centre.distance(tower.centre_metres()) <= 0.001
                            && radial.reference_outward.length_squared() > 0.99
                    }) && shell.is_some_and(|shell| {
                        matches!(
                            shell.shape,
                            crate::ResolvedSolidShape::RoundTowerShell {
                                outer_radius_metres,
                                inner_radius_metres,
                                chord_interfaces,
                            } if (outer_radius_metres - tower.radius_metres()).abs() <= 0.001
                                && (outer_radius_metres - inner_radius_metres
                                    - wall.thickness_metres).abs() <= 0.001
                                && chord_interfaces
                                    == [tower.chord_interface, tower.secondary_chord_interface]
                        )
                    })
                });
                if !valid {
                    issues.push(issue(
                        "invalid_round_wall_authority",
                        format!("round wall {} drifts from its grid tower shell", wall.id.0),
                    ));
                }
            }
            WallSourceId::StoreyWall { .. }
            | WallSourceId::CurtainWall { .. }
            | WallSourceId::ArtilleryCurtain { .. }
            | WallSourceId::SquareTowerFace { .. }
            | WallSourceId::CathedralClerestory { .. }
            | WallSourceId::RoofChildFront { .. }
            | WallSourceId::ChurchExterior { .. }
            | WallSourceId::ChurchArcade { .. }
            | WallSourceId::ChurchCrossing { .. }
            | WallSourceId::ChurchApse { .. }
            | WallSourceId::ChurchTowerFace { .. } => {
                if wall.radial_frame.is_some() {
                    issues.push(issue(
                        "invalid_wall_authority",
                        format!("linear wall {} declares a radial frame", wall.id.0),
                    ));
                }
            }
            WallSourceId::ArtilleryRondel { .. } => {}
        }
        if wall.host_solids.is_empty()
            || wall.host_solids.iter().any(|id| {
                solids.get(id).is_none_or(|solid| {
                    wall.replaced_by_owner
                        .map_or(solid.owner != wall.owner, |owner| solid.owner != owner)
                })
            })
        {
            issues.push(issue(
                "invalid_wall_host_union",
                format!("wall {} does not own an exact resolved host set", wall.id.0),
            ));
        }
        if !nodes.contains_key(&wall.support_node) {
            issues.push(issue(
                "unsupported_wall_assembly",
                format!("wall {} has no structural support node", wall.id.0),
            ));
        }
        if matches!(wall.source, WallSourceId::StoreyWall { .. })
            && wall.frame.outside_room.is_none()
            && wall.replaced_by_owner.is_none()
        {
            let expected_face =
                wall.frame.origin.dot(wall.frame.outward) + wall.thickness_metres * 0.5;
            let discontinuous = wall
                .host_solids
                .iter()
                .filter_map(|id| solids.get(id))
                .filter(|solid| {
                    matches!(
                        solid.role,
                        SolidRole::WallHost
                            | SolidRole::OpeningJamb
                            | SolidRole::OpeningSill
                            | SolidRole::OpeningHead
                            | SolidRole::OpeningSpandrel
                    ) && !(wall.material == WallMaterialClass::TimberInfill
                        && solid.role == SolidRole::WallHost)
                })
                .any(|solid| {
                    let centre = Vec2::new(solid.centre.x, solid.centre.z);
                    let radial_extent = wall.frame.outward.x.abs() * solid.size.x * 0.5
                        + wall.frame.outward.y.abs() * solid.size.z * 0.5;
                    (centre.dot(wall.frame.outward) + radial_extent - expected_face).abs() > 0.015
                });
            if discontinuous {
                issues.push(issue(
                    "discontinuous_exterior_wall_face",
                    format!(
                        "wall {} projects a displaced leaf/fin beyond its collinear exterior plane",
                        wall.id.0
                    ),
                ));
            }
        }
    }
    let mut opening_ids = std::collections::HashSet::new();
    for opening in &plan.opening_assemblies {
        let Some(wall) = plan
            .wall_assemblies
            .iter()
            .find(|wall| wall.id == opening.host_wall)
        else {
            issues.push(issue(
                "opening_without_host",
                format!("opening {} has no wall", opening.id.0),
            ));
            continue;
        };
        if !opening_ids.insert(opening.id)
            || opening.host_source != wall.source
            || !wall.opening_ids.contains(&opening.id)
        {
            issues.push(issue(
                "invalid_opening_authority",
                format!(
                    "opening {} is duplicated or drifts from its host",
                    opening.id.0
                ),
            ));
        }
        let Some(void) = plan
            .resolved_geometry
            .voids
            .iter()
            .find(|void| void.id == opening.void_id)
        else {
            issues.push(issue(
                "shallow_wall_opening",
                format!("opening {} has no void", opening.id.0),
            ));
            continue;
        };
        let depth = opening.frame.outward.x.abs() * (void.bounds.max.x - void.bounds.min.x)
            + opening.frame.outward.y.abs() * (void.bounds.max.z - void.bounds.min.z);
        if void.role != VoidRole::WallOpening
            || void.owner != opening.owner
            || void.subtracts_from != opening.owner
            || depth + 0.01 < wall.thickness_metres
        {
            issues.push(issue(
                "shallow_wall_opening",
                format!(
                    "opening {} is not a connected full-depth subtraction",
                    opening.id.0
                ),
            ));
        }
        let (
            profile_exterior_width,
            profile_interior_width,
            profile_exterior_height,
            profile_interior_height,
        ) = match opening.profile {
            OpeningProfile::ArrowLoop {
                exterior_width_metres,
                interior_width_metres,
                exterior_height_metres,
                interior_height_metres,
            }
            | OpeningProfile::GunLoop {
                exterior_width_metres,
                interior_width_metres,
                exterior_height_metres,
                interior_height_metres,
                ..
            } => (
                exterior_width_metres,
                interior_width_metres,
                exterior_height_metres,
                interior_height_metres,
            ),
            profile => (
                profile.exterior_width_metres(),
                profile.interior_width_metres(),
                profile.clear_height_metres(),
                profile.clear_height_metres(),
            ),
        };
        let expected_depth_sign = if opening.frame.tangent.x.abs() > 0.5 {
            if opening.frame.outward.y >= 0.0 {
                1
            } else {
                -1
            }
        } else if opening.frame.outward.x <= 0.0 {
            1
        } else {
            -1
        };
        let sectional_shape_matches = matches!(void.shape,
            crate::ResolvedVoidShape::SectionalOpening {
                opening: resolved_opening,
                exterior_width_metres,
                interior_width_metres,
                exterior_height_metres,
                interior_height_metres,
                exterior_depth_sign,
            } if resolved_opening == opening.id
                && (exterior_width_metres-profile_exterior_width).abs() <= 0.001
                && (interior_width_metres-profile_interior_width).abs() <= 0.001
                && (exterior_height_metres-profile_exterior_height.min(opening.profile.clear_height_metres())).abs() <= 0.001
                && (interior_height_metres-profile_interior_height.min(opening.profile.clear_height_metres())).abs() <= 0.001
                && exterior_depth_sign == expected_depth_sign);
        let slices_valid = opening.sectional_void.len() >= 5
            && opening.sectional_void.first().is_some_and(|slice| {
                slice.depth_fraction.abs() <= 0.001
                    && (slice.width_metres - profile_exterior_width).abs() <= 0.001
            })
            && opening.sectional_void.last().is_some_and(|slice| {
                (slice.depth_fraction - 1.0).abs() <= 0.001
                    && (slice.width_metres - profile_interior_width).abs() <= 0.001
            })
            && opening.sectional_void.windows(2).all(|pair| {
                pair[1].depth_fraction > pair[0].depth_fraction
                    && pair[1].width_metres + 0.001 >= pair[0].width_metres
                    && pair[1].height_metres + 0.001 >= pair[0].height_metres
            })
            && opening.sectional_void.iter().all(|slice| {
                let expected_width = profile_exterior_width
                    + (profile_interior_width - profile_exterior_width) * slice.depth_fraction;
                let expected_height = profile_exterior_height
                    + (profile_interior_height - profile_exterior_height) * slice.depth_fraction;
                (slice.width_metres - expected_width).abs() <= 0.002
                    && (slice.height_metres - expected_height).abs() <= 0.002
            });
        if !sectional_shape_matches || !slices_valid {
            issues.push(issue(
                "false_splayed_wall_opening",
                format!(
                    "opening {} lacks an ordered connected throat-to-mouth free-space field",
                    opening.id.0
                ),
            ));
        }
        let profile_valid = match opening.profile {
            OpeningProfile::Rectangular {
                width_metres,
                height_metres,
            } => {
                width_metres >= 0.68
                    && height_metres
                        >= if matches!(opening.host_source, WallSourceId::RoofChildFront { .. }) {
                            0.68
                        } else {
                            1.0
                        }
            }
            OpeningProfile::Segmental {
                width_metres,
                spring_height_metres,
                rise_metres,
                intrados_depth_metres,
            } => {
                width_metres >= 0.75
                    && spring_height_metres
                        >= if opening.use_kind == OpeningUse::Gate {
                            1.8
                        } else {
                            0.8
                        }
                    && rise_metres > 0.12
                    && intrados_depth_metres >= 0.12
            }
            OpeningProfile::PointedTwoCentred {
                width_metres,
                spring_height_metres,
                apex_height_metres,
                arc_radius_metres,
            } => {
                let half_span = width_metres * 0.5;
                let rise = apex_height_metres - spring_height_metres;
                let constructed_radius =
                    half_span + (rise * rise - half_span * half_span) / (2.0 * half_span.max(0.01));
                width_metres >= 0.35
                    && apex_height_metres > spring_height_metres + 0.40
                    && arc_radius_metres > width_metres * 0.5
                    && (arc_radius_metres - constructed_radius).abs() <= 0.01
            }
            OpeningProfile::ArrowLoop {
                exterior_width_metres,
                interior_width_metres,
                exterior_height_metres,
                interior_height_metres,
            } => {
                exterior_width_metres < interior_width_metres
                    && exterior_height_metres <= interior_height_metres
                    && exterior_width_metres <= 0.22
            }
            OpeningProfile::GunLoop {
                exterior_width_metres,
                interior_width_metres,
                exterior_height_metres,
                interior_height_metres,
                traverse_degrees,
                recoil_metres,
                crew_clearance_metres,
                ..
            } => {
                exterior_width_metres < interior_width_metres
                    && exterior_height_metres < interior_height_metres
                    && traverse_degrees >= 20.0
                    && recoil_metres >= 0.65
                    && crew_clearance_metres >= 1.0
            }
        };
        let use_profile_match = matches!(
            (opening.use_kind, opening.profile),
            (OpeningUse::Door, OpeningProfile::Rectangular { .. })
                | (OpeningUse::Gate, OpeningProfile::Segmental { .. })
                | (
                    OpeningUse::Window,
                    OpeningProfile::Rectangular { .. }
                        | OpeningProfile::Segmental { .. }
                        | OpeningProfile::PointedTwoCentred { .. }
                )
                | (OpeningUse::ArrowLoop, OpeningProfile::ArrowLoop { .. })
                | (OpeningUse::GunLoop, OpeningProfile::GunLoop { .. })
                | (
                    OpeningUse::BellOpening,
                    OpeningProfile::PointedTwoCentred { .. }
                )
        );
        if !profile_valid || !use_profile_match {
            issues.push(issue(
                "invalid_opening_profile",
                format!(
                    "opening {} has an invalid or substituted section",
                    opening.id.0
                ),
            ));
        }
        let exact_piece = |id: ResolvedItemId, role: SolidRole| {
            solids
                .get(&id)
                .is_some_and(|solid| solid.owner == opening.owner && solid.role == role)
        };
        if !exact_piece(opening.jamb_solids[0], SolidRole::OpeningJamb)
            || !exact_piece(opening.jamb_solids[1], SolidRole::OpeningJamb)
            || !exact_piece(opening.head_solid, SolidRole::OpeningHead)
            || !exact_piece(opening.spandrel_solid, SolidRole::OpeningSpandrel)
            || opening.reveal_surfaces.len() < 6
            || opening.reveal_surfaces.iter().any(|id| {
                surfaces.get(id).is_none_or(|surface| {
                    surface.owner != opening.owner
                        || !matches!(
                            surface.role,
                            SurfaceRole::LeftJambReveal
                                | SurfaceRole::RightJambReveal
                                | SurfaceRole::WeatherSill
                                | SurfaceRole::Intrados
                                | SurfaceRole::ExteriorThroat
                                | SurfaceRole::InteriorMouth
                        )
                })
            })
        {
            issues.push(issue(
                "missing_opening_reveal_piece",
                format!("opening {} lacks exact jamb/head/reveal IDs", opening.id.0),
            ));
        }
        let opening_offset = (opening.frame.origin - wall.frame.origin).dot(opening.frame.tangent);
        let exterior_width_for_layout = opening.profile.exterior_width_metres();
        let jambs_on_declared_reveals =
            opening.jamb_solids.iter().enumerate().all(|(index, id)| {
                let side = if index == 0 { -1.0_f32 } else { 1.0 };
                let side_width = if side < 0.0 {
                    wall.length_metres * 0.5 + opening_offset - exterior_width_for_layout * 0.5
                } else {
                    wall.length_metres * 0.5 - opening_offset - exterior_width_for_layout * 0.5
                };
                solids.get(id).is_some_and(|solid| {
                    let expected = opening.frame.origin
                        + opening.frame.tangent
                            * side
                            * (exterior_width_for_layout + side_width)
                            * 0.5;
                    Vec2::new(solid.centre.x, solid.centre.z).distance(expected) <= 0.015
                })
            });
        if !jambs_on_declared_reveals {
            issues.push(issue(
                "false_opening_head_load_path",
                format!(
                    "opening {} jambs drift from their measured springing/reveal lines",
                    opening.id.0
                ),
            ));
        }
        let splayed_profile = match opening.profile {
            OpeningProfile::ArrowLoop {
                exterior_width_metres,
                interior_width_metres,
                exterior_height_metres,
                interior_height_metres,
                ..
            }
            | OpeningProfile::GunLoop {
                exterior_width_metres,
                interior_width_metres,
                exterior_height_metres,
                interior_height_metres,
                ..
            } => Some((
                exterior_width_metres,
                interior_width_metres,
                exterior_height_metres,
                interior_height_metres,
            )),
            _ => None,
        };
        if let Some((exterior_width, interior_width, exterior_height, interior_height)) =
            splayed_profile
        {
            let exact_splayed_jamb = |id: ResolvedItemId, expected_side: i8| {
                solids.get(&id).is_some_and(|solid| {
                    matches!(
                        solid.shape,
                        crate::ResolvedSolidShape::SplayedReveal {
                            exterior_width_metres,
                            interior_width_metres,
                            side,
                            exterior_depth_sign,
                        } if (exterior_width_metres - exterior_width).abs() <= 0.001
                            && (interior_width_metres - interior_width).abs() <= 0.001
                            && side == expected_side
                            && exterior_depth_sign == expected_depth_sign
                    )
                })
            };
            let tangent_depth = opening.frame.tangent.x.abs()
                * (void.bounds.max.x - void.bounds.min.x)
                + opening.frame.tangent.y.abs() * (void.bounds.max.z - void.bounds.min.z);
            let exact_splayed_head = solids.get(&opening.head_solid).is_some_and(|solid| {
                matches!(
                    solid.shape,
                    crate::ResolvedSolidShape::SplayedHead {
                        exterior_clear_height_metres,
                        interior_clear_height_metres,
                        exterior_depth_sign,
                    } if (exterior_clear_height_metres - exterior_height).abs() <= 0.001
                        && (interior_clear_height_metres - interior_height).abs() <= 0.001
                        && exterior_depth_sign == expected_depth_sign
                )
            });
            let sampled_host = opening.sectional_void.iter().all(|slice| {
                let plan = opening.frame.origin
                    + opening.frame.outward
                        * (wall.thickness_metres * (0.5 - slice.depth_fraction));
                let clear_top = opening.sill_elevation_metres + slice.height_metres;
                let free_point = Vec3::new(plan.x, clear_top - 0.015, plan.y);
                let head_point = Vec3::new(plan.x, clear_top + 0.015, plan.y);
                let side_height = opening.sill_elevation_metres + slice.height_metres * 0.5;
                let side_offset = slice.width_metres * 0.5 + 0.015;
                let side_points = [-1.0_f32, 1.0].map(|side| {
                    let side_plan = plan + opening.frame.tangent * side * side_offset;
                    Vec3::new(side_plan.x, side_height, side_plan.y)
                });
                let host_solids = [
                    opening.jamb_solids[0],
                    opening.jamb_solids[1],
                    opening.head_solid,
                    opening.spandrel_solid,
                ];
                let contains = |point| {
                    host_solids.iter().any(|id| {
                        solids.get(id).is_some_and(|solid| {
                            opening_host_contains_point(opening, wall, solid, point)
                        })
                    })
                };
                !contains(free_point)
                    && contains(head_point)
                    && side_points.into_iter().all(contains)
            });
            if !exact_splayed_jamb(opening.jamb_solids[0], -1)
                || !exact_splayed_jamb(opening.jamb_solids[1], 1)
                || !exact_splayed_head
                || !sampled_host
                || (tangent_depth - interior_width).abs() > 0.02
            {
                issues.push(issue(
                    "false_splayed_wall_opening",
                    format!(
                        "opening {} does not resolve its sampled narrow throat, broad mouth, and rising head into physical host masonry",
                        opening.id.0
                    ),
                ));
            }
        } else if opening.jamb_solids.iter().any(|id| {
            solids.get(id).is_some_and(|solid| {
                matches!(solid.shape, crate::ResolvedSolidShape::SplayedReveal { .. })
            })
        }) {
            issues.push(issue(
                "false_splayed_wall_opening",
                format!(
                    "opening {} substitutes a splay for its declared profile",
                    opening.id.0
                ),
            ));
        }
        let resolved_surface_roles = opening
            .reveal_surfaces
            .iter()
            .filter_map(|id| {
                plan.resolved_geometry
                    .surfaces
                    .iter()
                    .find(|surface| surface.id == *id && surface.owner == opening.owner)
                    .map(|surface| surface.role)
            })
            .collect::<Vec<_>>();
        if !resolved_surface_roles.contains(&crate::SurfaceRole::LeftJambReveal)
            || !resolved_surface_roles.contains(&crate::SurfaceRole::RightJambReveal)
            || !resolved_surface_roles.contains(&crate::SurfaceRole::WeatherSill)
            || !resolved_surface_roles.contains(&crate::SurfaceRole::Intrados)
            || !resolved_surface_roles.contains(&crate::SurfaceRole::ExteriorThroat)
            || !resolved_surface_roles.contains(&crate::SurfaceRole::InteriorMouth)
        {
            issues.push(issue(
                "missing_opening_reveal_surface",
                format!(
                    "opening {} lacks exact reveal, weather-sill, or intrados surfaces",
                    opening.id.0
                ),
            ));
        }
        // Surface identity belongs to the opening's exact reveal multiset.
        // Several structural bays may legitimately share a church assembly
        // owner, so owner+role alone can select a neighbouring sill/intrados.
        let shaped_surface = |role| {
            opening.reveal_surfaces.iter().find_map(|id| {
                surfaces
                    .get(id)
                    .filter(|surface| surface.owner == opening.owner && surface.role == role)
                    .copied()
            })
        };
        let sill_and_intrados_valid = shaped_surface(crate::SurfaceRole::WeatherSill).is_some_and(|surface| matches!(surface.shape,
                crate::ResolvedSurfaceShape::WeatherSill { interior_elevation_metres, exterior_elevation_metres, drip_depth_metres }
                    if exterior_elevation_metres + 0.02 < interior_elevation_metres && drip_depth_metres >= 0.02))
            && shaped_surface(crate::SurfaceRole::Intrados).is_some_and(|surface| match (opening.profile, surface.shape) {
                (OpeningProfile::Segmental { width_metres, spring_height_metres, rise_metres, .. }, crate::ResolvedSurfaceShape::SegmentalIntrados { clear_span_metres, spring_height_metres: spring, rise_metres: rise }) => (clear_span_metres-width_metres).abs() <= 0.001 && (spring-spring_height_metres).abs() <= 0.001 && (rise-rise_metres).abs() <= 0.001,
                (OpeningProfile::PointedTwoCentred { width_metres, spring_height_metres, apex_height_metres, arc_radius_metres }, crate::ResolvedSurfaceShape::PointedIntrados { clear_span_metres, spring_height_metres: spring, apex_height_metres: apex, arc_radius_metres: radius }) => (clear_span_metres-width_metres).abs() <= 0.001 && (spring-spring_height_metres).abs() <= 0.001 && (apex-apex_height_metres).abs() <= 0.001 && (radius-arc_radius_metres).abs() <= 0.001,
                (OpeningProfile::Rectangular { .. } | OpeningProfile::ArrowLoop { .. } | OpeningProfile::GunLoop { .. }, crate::ResolvedSurfaceShape::Planar) => true,
                _ => false,
            });
        if !sill_and_intrados_valid {
            issues.push(issue(
                "invalid_opening_weather_or_intrados",
                format!(
                    "opening {} has a flat/uphill sill or substituted intrados",
                    opening.id.0
                ),
            ));
        }
        let head = nodes.get(&opening.head_node);
        let spandrel_node = nodes.get(&opening.spandrel_node);
        if head.is_none_or(|head| {
            head.kind != crate::StructuralNodeKind::OpeningHead
                || head.supported_by.len() != 2
                || !opening
                    .jamb_nodes
                    .iter()
                    .all(|jamb| head.supported_by.contains(jamb))
        }) || opening.jamb_nodes.iter().any(|jamb| {
            nodes.get(jamb).is_none_or(|node| {
                node.kind != crate::StructuralNodeKind::OpeningJamb
                    || !node.supported_by.contains(&wall.support_node)
            })
        }) || spandrel_node.is_none_or(|node| {
            node.kind != crate::StructuralNodeKind::OpeningSpandrel
                || node.supported_by != [opening.head_node]
        }) {
            issues.push(issue(
                "false_opening_head_load_path",
                format!(
                    "opening {} head does not bear through two grounded jambs",
                    opening.id.0
                ),
            ));
        }
        let head_solid = solids.get(&opening.head_solid);
        let spandrel_solid = solids.get(&opening.spandrel_solid);
        let bearing_interfaces = opening.head_bearing_interfaces.map(|id| {
            plan.resolved_geometry
                .support_interfaces
                .iter()
                .find(|interface| {
                    interface.id == id
                        && interface.owner == opening.owner
                        && interface.node == opening.head_node
                })
        });
        let wall_above = plan
            .resolved_geometry
            .support_interfaces
            .iter()
            .find(|interface| {
                interface.id == opening.wall_above_interface
                    && interface.owner == opening.owner
                    && interface.node == opening.spandrel_node
            });
        let contact_valid =
            head_solid.is_some_and(|head_solid| {
                bearing_interfaces.into_iter().zip(opening.jamb_solids).all(
                    |(interface, jamb_id)| {
                        let Some(interface) = interface else {
                            return false;
                        };
                        let Some(jamb) = solids.get(&jamb_id) else {
                            return false;
                        };
                        let (head_min, head_max) = resolved_solid_bounds(head_solid);
                        let (jamb_min, jamb_max) = resolved_solid_bounds(jamb);
                        let contact_min = head_min.max(jamb_min).max(interface.bounds.min);
                        let contact_max = head_max.min(jamb_max).min(interface.bounds.max);
                        let size = contact_max - contact_min;
                        size.min_element() > 0.001 && {
                            let mut extents = [size.x, size.y, size.z];
                            extents.sort_by(f32::total_cmp);
                            extents[1] * extents[2] >= 0.01
                        }
                    },
                ) && spandrel_solid.is_some_and(|spandrel| {
                    spandrel.supported_by == [opening.spandrel_node]
                        && wall_above.is_some_and(|interface| {
                            let (head_min, head_max) = resolved_solid_bounds(head_solid);
                            let (spandrel_min, spandrel_max) = resolved_solid_bounds(spandrel);
                            let contact_min = head_min.max(spandrel_min).max(interface.bounds.min);
                            let contact_max = head_max.min(spandrel_max).min(interface.bounds.max);
                            let size = contact_max - contact_min;
                            size.min_element() > 0.001 && {
                                let mut extents = [size.x, size.y, size.z];
                                extents.sort_by(f32::total_cmp);
                                extents[1] * extents[2] >= 0.02
                            }
                        })
                })
            });
        if !contact_valid {
            issues.push(issue("false_opening_head_load_path", format!("opening {} head lacks measured two-ended bearing or distinct upper-spandrel contact; head={:?} spandrel={:?} bearings={:?} wall_above={:?}", opening.id.0, head_solid.map(|solid| (solid.centre, solid.size)), spandrel_solid.map(|solid| (solid.centre, solid.size)), bearing_interfaces.map(|interface| interface.map(|interface| (interface.bounds.min, interface.bounds.max))), wall_above.map(|interface| (interface.bounds.min, interface.bounds.max)))));
        }
        let wide_cathedral_light = opening.use_kind == OpeningUse::Window
            && matches!(opening.profile, OpeningProfile::PointedTwoCentred { width_metres, .. } if width_metres >= 0.90);
        if wide_cathedral_light {
            let tracery_node = opening.tracery_node.and_then(|id| nodes.get(&id));
            let tracery_solids = plan
                .resolved_geometry
                .solids
                .iter()
                .filter(|solid| {
                    solid.owner == opening.owner
                        && solid.role == SolidRole::Mullion
                        && opening
                            .tracery_node
                            .is_some_and(|node| solid.supported_by == [node])
                })
                .collect::<Vec<_>>();
            let bearing = opening.tracery_node.and_then(|node| {
                plan.resolved_geometry
                    .support_interfaces
                    .iter()
                    .find(|interface| interface.owner == opening.owner && interface.node == node)
            });
            let sill = opening.sill_solid.and_then(|id| solids.get(&id));
            let mullion_bears = tracery_solids.first().is_some_and(|mullion| {
                sill.is_some_and(|sill| {
                    bearing.is_some_and(|interface| {
                        let (mullion_min, mullion_max) = resolved_solid_bounds(mullion);
                        let (sill_min, sill_max) = resolved_solid_bounds(sill);
                        let contact_min = mullion_min.max(sill_min).max(interface.bounds.min);
                        let contact_max = mullion_max.min(sill_max).min(interface.bounds.max);
                        let size = contact_max - contact_min;
                        size.min_element() > 0.001 && size.x.max(size.z) >= 0.06
                    })
                })
            });
            if tracery_node.is_none_or(|node| {
                node.kind != crate::StructuralNodeKind::MullionBearing
                    || node.supported_by != [wall.support_node]
            }) || tracery_solids.len() < 2
                || !mullion_bears
                || opening.closure_solids.len() < 2
                || opening.closure_solids.iter().any(|id| {
                    solids
                        .get(id)
                        .is_none_or(|solid| solid.role != SolidRole::LeadedGlazing)
                })
            {
                issues.push(issue(
                    "unsupported_cathedral_tracery",
                    format!(
                        "opening {} lacks stone mullion/transom bearing or subdivided glazing",
                        opening.id.0
                    ),
                ));
            }
        } else if opening.tracery_node.is_some() {
            issues.push(issue(
                "unsupported_cathedral_tracery",
                format!(
                    "opening {} declares tracery outside a principal cathedral light",
                    opening.id.0
                ),
            ));
        }
        let illegal_closure = match opening.use_kind {
            OpeningUse::ArrowLoop | OpeningUse::GunLoop => {
                opening.closure.layers != [ClosureKind::OpenMilitary]
                    || !opening.closure_solids.is_empty()
            }
            OpeningUse::Window if plan.archetype == BuildingArchetype::Cathedral => {
                opening.closure.layers != [ClosureKind::LeadedGlazing]
            }
            OpeningUse::Window => !opening.closure.layers.contains(&ClosureKind::TimberShutter),
            OpeningUse::Door | OpeningUse::Gate => {
                !opening.closure.layers.contains(&ClosureKind::DoorLeaf)
            }
            OpeningUse::BellOpening => opening.closure.layers != [ClosureKind::TimberLouvre],
        };
        if illegal_closure {
            issues.push(issue(
                "illegal_opening_closure",
                format!(
                    "opening {} has an illegal glazing/closure policy",
                    opening.id.0
                ),
            ));
        }
        if matches!(
            opening.use_kind,
            OpeningUse::ArrowLoop | OpeningUse::GunLoop
        ) && (opening.stance_surface.is_none()
            || opening.ray_indices.len() != 3
            || opening.ray_indices.iter().any(|index| {
                plan.resolved_geometry
                    .projected_defense_rays
                    .get(*index)
                    .is_none_or(|ray| ray.owner != opening.owner || ray.throat != opening.void_id)
            })
            || (opening.use_kind == OpeningUse::GunLoop && opening.mount_solid.is_none()))
        {
            issues.push(issue(
                "inoperable_military_opening",
                format!(
                    "opening {} lacks stance/mount/near-mid-far rays",
                    opening.id.0
                ),
            ));
        }
        let head_shape = solids.get(&opening.head_solid).map(|solid| solid.shape);
        let head_matches = match opening.profile {
            OpeningProfile::Segmental {
                width_metres,
                spring_height_metres,
                rise_metres,
                intrados_depth_metres,
            } => {
                opening.head_kind == OpeningHeadKind::SegmentalArch
                    && matches!(
                        head_shape,
                        Some(crate::ResolvedSolidShape::SegmentalArchRing {
                            clear_span_metres,
                            spring_height_metres: resolved_spring,
                            rise_metres: resolved_rise,
                            ring_depth_metres,
                        }) if (clear_span_metres - width_metres).abs() <= 0.001
                            && (resolved_spring - spring_height_metres).abs() <= 0.001
                            && (resolved_rise - rise_metres).abs() <= 0.001
                            && (ring_depth_metres - intrados_depth_metres).abs() <= 0.001
                    )
            }
            OpeningProfile::PointedTwoCentred {
                width_metres,
                spring_height_metres,
                apex_height_metres,
                arc_radius_metres,
            } => {
                opening.head_kind == OpeningHeadKind::PointedVoussoir
                    && matches!(
                        head_shape,
                        Some(crate::ResolvedSolidShape::PointedArchRing {
                            clear_span_metres,
                            spring_height_metres: resolved_spring,
                            apex_height_metres: resolved_apex,
                            arc_radius_metres: resolved_radius,
                            ..
                        }) if (clear_span_metres - width_metres).abs() <= 0.001
                            && (resolved_spring - spring_height_metres).abs() <= 0.001
                            && (resolved_apex - apex_height_metres).abs() <= 0.001
                            && (resolved_radius - arc_radius_metres).abs() <= 0.001
                    )
            }
            OpeningProfile::Rectangular { .. } => matches!(
                opening.head_kind,
                OpeningHeadKind::TimberLintel | OpeningHeadKind::StoneLintel
            ),
            OpeningProfile::ArrowLoop {
                exterior_height_metres,
                interior_height_metres,
                ..
            }
            | OpeningProfile::GunLoop {
                exterior_height_metres,
                interior_height_metres,
                ..
            } => matches!(
                (opening.head_kind, head_shape),
                (
                    OpeningHeadKind::StoneLintel,
                    Some(crate::ResolvedSolidShape::SplayedHead {
                        exterior_clear_height_metres,
                        interior_clear_height_metres,
                        ..
                    })
                ) if (exterior_clear_height_metres - exterior_height_metres).abs() <= 0.001
                    && (interior_clear_height_metres - interior_height_metres).abs() <= 0.001
            ),
        };
        if !head_matches {
            issues.push(issue(
                "opening_head_profile_mismatch",
                format!("opening {} head does not match its section", opening.id.0),
            ));
        }
    }
    let source_openings = plan
        .storeys
        .iter()
        .map(|storey| storey.openings.len())
        .sum::<usize>();
    let replaced_openings = plan
        .wall_assemblies
        .iter()
        .filter(|wall| wall.replaced_by_owner.is_some())
        .filter(|wall| match wall.source {
            WallSourceId::StoreyWall {
                storey_level,
                wall_index,
            } => plan
                .storeys
                .get(storey_level as usize)
                .is_some_and(|storey| {
                    storey
                        .openings
                        .iter()
                        .any(|opening| opening.wall == wall_index)
                }),
            _ => false,
        })
        .count();
    let bell_openings = plan
        .square_towers
        .iter()
        .filter(|tower| tower.bell_openings)
        .count()
        * 8;
    let roof_child_openings = plan.roof_dormers.len();
    let church_portals = usize::from(plan.church.is_some()) * 2;
    let church_windows = plan.church.as_ref().map_or(0, |church| {
        usize::from(church.program.nave_bays) * 4
            + usize::from(church.program.choir_bays) * 2
            + 2
            + usize::from(church.program.apse_sides.saturating_sub(1))
    });
    let artillery_openings = plan
        .artillery_castle
        .as_ref()
        .map_or(0, |castle| castle.stations.len());
    if plan.opening_assemblies.len() + replaced_openings
        != source_openings
            + bell_openings
            + roof_child_openings
            + church_portals
            + church_windows
            + artillery_openings
    {
        issues.push(issue(
            "legacy_opening_not_migrated",
            format!(
                "resolved {} of {} openings",
                plan.opening_assemblies.len(),
                source_openings
                    + bell_openings
                    + roof_child_openings
                    + church_portals
                    + church_windows
                    + artillery_openings
            ),
        ));
    }
}
