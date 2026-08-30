fn spawn_resolved_crowns(
    world: &mut World,
    palette: &RenderPalette,
    plan: &BuildingPlan,
    origin: Vec2,
    visible_owners: Option<&std::collections::HashSet<u32>>,
    section_view: Option<ViewerView>,
) {
    let removed_items = section_view
        .map(|view| {
            architectural_section_removed_item_ids(plan, view)
                .into_iter()
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    let isolated_church_items = section_view
        .filter(|view| {
            matches!(
                view,
                ViewerView::ChurchBayInterior
                    | ViewerView::ChurchBaySection
                    | ViewerView::ChurchBayLoad
                    | ViewerView::ChurchBayVault
                    | ViewerView::ChurchCrossingInterior
                    | ViewerView::ChurchCrossingCutLoad
                    | ViewerView::ChurchChoirInterior
                    | ViewerView::ChurchChoirRadialSection
                    | ViewerView::ChurchTowerStair
                    | ViewerView::ChurchTowerBellUnderside
                    | ViewerView::ChurchTowerFrame
                    | ViewerView::ChurchDrainage
                    | ViewerView::ChurchSupportDag
            )
        })
        .map(|view| {
            church_focus_item_ids(plan, view)
                .into_iter()
                .collect::<std::collections::HashSet<_>>()
        });
    let isolated_timber_items =
        section_view
            .filter(|view| timber_isolated_view(*view))
            .map(|view| {
                timber_focus_item_ids(plan, view)
                    .into_iter()
                    .collect::<std::collections::HashSet<_>>()
            });
    for solid in &plan.resolved_geometry.solids {
        if removed_items.contains(&solid.id.0) {
            continue;
        }
        if isolated_timber_items
            .as_ref()
            .is_some_and(|items| !items.contains(&solid.id.0))
        {
            continue;
        }
        // Round shells are emitted by `spawn_tower`, which consumes the same
        // resolved solid ID while also applying its authoritative portal and
        // firing-loop subtractions. Spawning the envelope again here would
        // duplicate the masonry volume.
        if matches!(
            solid.shape,
            adventuresim_building_generator::ResolvedSolidShape::RoundTowerShell { .. }
        ) {
            continue;
        }
        let projected = plan
            .projected_defenses
            .iter()
            .find(|defense| defense.owner == solid.owner || defense.host_owner == solid.owner);
        let wall = plan
            .wall_assemblies
            .iter()
            .find(|wall| wall.host_solids.contains(&solid.id))
            .or_else(|| {
                plan.wall_assemblies.iter().find(|wall| {
                    wall.owner == solid.owner || wall.replaced_by_owner == Some(solid.owner)
                })
            });
        let material = if section_view == Some(ViewerView::TimberRegistrationCut)
            && solid.role == SolidRole::FrameFloor
            || matches!(
                section_view,
                Some(ViewerView::TimberOpeningBayInterior | ViewerView::TimberOpeningBaySection)
            ) && solid.role == SolidRole::WallHost
            || section_view == Some(ViewerView::TimberTownHallJunction)
                && solid.role == SolidRole::WallHost
        {
            &palette.cutaway
        } else if section_view == Some(ViewerView::ArtilleryRondelCasemate)
            && solid.role == SolidRole::ArtilleryEarthCore
        {
            // Preserve the authoritative residual mass in the casemate proof
            // while allowing its enclosed station, recoil area, smoke path,
            // and spiral access to be read simultaneously.
            &palette.cutaway
        } else if section_view == Some(ViewerView::ArtilleryCurtainSection)
            && solid.role == SolidRole::ArtilleryRetainingWall
        {
            // Section-only material separation: the authority remains
            // fieldstone masonry, while the warmer proof color makes the
            // inner retaining leaf distinguishable from the pale revetment.
            &palette.brick
        } else if section_view == Some(ViewerView::ArtilleryGateInterior)
            && solid.role == SolidRole::ArtilleryGateMechanism
        {
            // A warmer, lighter structural-timber swatch distinguishes the
            // windlass drum and rope from the deep chamber recess while
            // preserving the assembly's authoritative material semantics.
            &palette.stair
        } else {
            match solid.role {
                SolidRole::EdgeGuard
                | SolidRole::FrameMember
                | SolidRole::FrameSill
                | SolidRole::FramePost
                | SolidRole::FramePlate
                | SolidRole::FrameRail
                | SolidRole::FrameJoist
                | SolidRole::FrameGirder
                | SolidRole::FrameTie
                | SolidRole::FrameBrace
                | SolidRole::FrameJettyBeam
                | SolidRole::FrameKnagge
                | SolidRole::FrameGableMember
                | SolidRole::FrameDormerTrimmer
                | SolidRole::FrameOrnament
                | SolidRole::BeamJoist
                | SolidRole::RoofFraming
                | SolidRole::RoofPlate
                | SolidRole::ArtilleryBridgeBeam
                | SolidRole::ArtilleryBridgeDeck
                | SolidRole::ArtilleryGateMechanism => &palette.timber,
                SolidRole::ArtilleryEarthCore
                | SolidRole::DitchFloor
                | SolidRole::DitchScarp
                | SolidRole::DitchCounterscarp => &palette.earth,
                SolidRole::FrameInfill => match plan.wall_style {
                    WallStyle::Brick => &palette.brick,
                    WallStyle::Stone => &palette.stone,
                    WallStyle::TimberFrame | WallStyle::Plaster => &palette.plaster,
                },
                SolidRole::FrameFloor
                | SolidRole::WalkSurface
                | SolidRole::DrainageChannel
                | SolidRole::DrainageFloor
                | SolidRole::GalleryFloor
                | SolidRole::Landing
                | SolidRole::CircuitWalk
                | SolidRole::ChurchFloor
                | SolidRole::ChurchBellFloor
                | SolidRole::ChurchVaultShell => &palette.floor,
                SolidRole::ChurchStairTread | SolidRole::ArtilleryStairTread => &palette.stair,
                SolidRole::ChurchStairNewel | SolidRole::ChurchServiceLadder => &palette.timber,
                SolidRole::RoofFlashing if solid.size.y <= 0.03 && solid.size.z <= 0.12 => {
                    &palette.roof
                }
                SolidRole::DefenseRoof | SolidRole::RoofFlashing | SolidRole::RoofGutter => {
                    &palette.roof_secondary
                }
                SolidRole::ProjectionSupport
                    if projected.is_some_and(|defense| {
                        defense.material
                            == adventuresim_building_generator::ProjectedDefenseMaterial::Timber
                    }) =>
                {
                    &palette.timber
                }
                SolidRole::OpeningClosure
                | SolidRole::WeaponMount
                | SolidRole::ChurchBellFrame
                | SolidRole::ChurchGuard => &palette.timber,
                SolidRole::ChurchBell => &palette.roof_secondary,
                SolidRole::Mullion => &palette.stone,
                SolidRole::LeadedGlazing => &palette.glass,
                SolidRole::WallHost
                | SolidRole::OpeningJamb
                | SolidRole::OpeningSill
                | SolidRole::OpeningHead
                | SolidRole::OpeningSpandrel => match wall.map(|wall| wall.material) {
                    Some(adventuresim_building_generator::WallMaterialClass::TimberInfill) => {
                        match plan.wall_style {
                            WallStyle::Brick => &palette.brick,
                            WallStyle::Stone => &palette.stone,
                            WallStyle::TimberFrame | WallStyle::Plaster => &palette.plaster,
                        }
                    }
                    Some(adventuresim_building_generator::WallMaterialClass::InternalTimber) => {
                        &palette.timber
                    }
                    Some(adventuresim_building_generator::WallMaterialClass::CivilianMasonry) => {
                        match plan.wall_style {
                            WallStyle::Brick => &palette.brick,
                            WallStyle::Plaster | WallStyle::TimberFrame => &palette.plaster,
                            WallStyle::Stone => &palette.stone,
                        }
                    }
                    _ => &palette.stone,
                },
                _ => &palette.stone,
            }
        };
        // Thick military walls can be deeper than one of their side piers, so
        // size comparison is not an orientation authority. Use the owning
        // wall's local frame; otherwise a 1.2 m wall rotates narrow jambs by
        // 90 degrees and creates the full-storey exterior fins seen in the
        // courtyard regression.
        let tangent_is_z = wall
            .map(|wall| wall.frame.tangent.y.abs() > 0.5)
            .unwrap_or(solid.size.z > solid.size.x);
        let (mesh, shape_yaw) = match solid.shape {
            adventuresim_building_generator::ResolvedSolidShape::SegmentalArchRing {
                spring_height_metres,
                rise_metres,
                ..
            } => (
                if matches!(
                    solid.role,
                    SolidRole::OpeningClosure | SolidRole::LeadedGlazing
                ) {
                    arched_panel_mesh(
                        solid.size.x.max(solid.size.z),
                        solid.size.y,
                        solid.size.x.min(solid.size.z),
                        spring_height_metres,
                        rise_metres,
                        None,
                    )
                } else {
                    arched_spandrel_mesh(
                        solid.size.x.max(solid.size.z),
                        solid.size.y,
                        solid.size.x.min(solid.size.z),
                        rise_metres,
                        None,
                    )
                },
                tangent_is_z.then_some(std::f32::consts::FRAC_PI_2),
            ),
            adventuresim_building_generator::ResolvedSolidShape::PointedArchRing {
                spring_height_metres,
                apex_height_metres,
                arc_radius_metres,
                ..
            } => (
                if matches!(
                    solid.role,
                    SolidRole::OpeningClosure | SolidRole::LeadedGlazing
                ) {
                    arched_panel_mesh(
                        solid.size.x.max(solid.size.z),
                        solid.size.y,
                        solid.size.x.min(solid.size.z),
                        spring_height_metres,
                        apex_height_metres - spring_height_metres,
                        Some(arc_radius_metres),
                    )
                } else {
                    arched_spandrel_mesh(
                        solid.size.x.max(solid.size.z),
                        solid.size.y,
                        solid.size.x.min(solid.size.z),
                        apex_height_metres - spring_height_metres,
                        Some(arc_radius_metres),
                    )
                },
                tangent_is_z.then_some(std::f32::consts::FRAC_PI_2),
            ),
            adventuresim_building_generator::ResolvedSolidShape::TimberPanelPrism {
                vertices,
                outward,
                depth_metres,
            } => (
                timber_panel_prism_mesh(vertices, outward, depth_metres, solid.centre),
                None,
            ),
            adventuresim_building_generator::ResolvedSolidShape::SplayedReveal {
                exterior_width_metres,
                interior_width_metres,
                side,
                exterior_depth_sign,
            } => (
                splayed_jamb_mesh(
                    solid.size.x.max(solid.size.z),
                    solid.size.y,
                    solid.size.x.min(solid.size.z),
                    exterior_width_metres,
                    interior_width_metres,
                    side,
                    exterior_depth_sign,
                ),
                tangent_is_z.then_some(-std::f32::consts::FRAC_PI_2),
            ),
            adventuresim_building_generator::ResolvedSolidShape::SplayedHead {
                exterior_clear_height_metres,
                interior_clear_height_metres,
                exterior_depth_sign,
            } => (
                splayed_head_mesh(
                    solid.size.x.max(solid.size.z),
                    solid.size.y,
                    solid.size.x.min(solid.size.z),
                    exterior_clear_height_metres,
                    interior_clear_height_metres,
                    exterior_depth_sign,
                ),
                tangent_is_z.then_some(-std::f32::consts::FRAC_PI_2),
            ),
            adventuresim_building_generator::ResolvedSolidShape::Cuboid => (
                Mesh::from(Cuboid::new(solid.size.x, solid.size.y, solid.size.z)),
                None,
            ),
            adventuresim_building_generator::ResolvedSolidShape::AnnularPrism {
                inner_radius_metres,
                outer_radius_metres,
                inner_top_offset_metres,
                outer_top_offset_metres,
                drainage_outlet_count,
                circumferential_fall_metres,
            } => (
                sloped_annulus_mesh(
                    inner_radius_metres,
                    outer_radius_metres,
                    solid.size.y,
                    inner_top_offset_metres,
                    outer_top_offset_metres,
                    drainage_outlet_count,
                    circumferential_fall_metres,
                ),
                None,
            ),
            adventuresim_building_generator::ResolvedSolidShape::AnnularSectorPrism {
                inner_radius_metres,
                outer_radius_metres,
                start_angle_radians,
                end_angle_radians,
                inner_top_offset_metres,
                outer_top_offset_metres,
            } => (
                annular_sector_mesh(
                    inner_radius_metres,
                    outer_radius_metres,
                    solid.size.y,
                    start_angle_radians,
                    end_angle_radians,
                    inner_top_offset_metres,
                    outer_top_offset_metres,
                ),
                None,
            ),
            adventuresim_building_generator::ResolvedSolidShape::RoundTowerShell { .. } => {
                unreachable!("round shells are rendered by spawn_tower")
            }
        };
        let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh);
        let resolved_yaw = if matches!(
            solid.role,
            SolidRole::RoofFraming
                | SolidRole::RoofFlashing
                | SolidRole::RoofGutter
                | SolidRole::RoofEdgeTreatment
        ) {
            -solid.yaw_radians
        } else {
            solid.yaw_radians
        };
        world.spawn((
            Name::new(if projected.is_some() {
                format!(
                    "resolved projected owner {} {:?}",
                    solid.owner.0, solid.role
                )
            } else {
                format!("resolved crown owner {} {:?}", solid.owner.0, solid.role)
            }),
            ClosedSolid,
            GeometryOwner(solid.owner.0),
            ResolvedRenderItem {
                id: solid.id.0,
                fingerprint: stable_u64(
                    &serde_json::to_vec(solid).expect("serialize rendered resolved solid"),
                ),
                local_half_size: solid.size * 0.5,
            },
            Mesh3d(mesh),
            MeshMaterial3d(material.clone()),
            Transform {
                translation: solid.centre + Vec3::new(origin.x, 0.0, origin.y),
                rotation: Quat::from_rotation_y(resolved_yaw)
                    * Quat::from_rotation_y(shape_yaw.unwrap_or(0.0))
                    * Quat::from_rotation_x(solid.crossfall_radians)
                    * Quat::from_rotation_z(solid.longfall_radians),
                ..default()
            },
            if isolated_church_items.as_ref().map_or_else(
                || visible_owners.is_none_or(|visible| visible.contains(&solid.owner.0)),
                |items| items.contains(&solid.id.0),
            ) {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
        ));
    }
}
