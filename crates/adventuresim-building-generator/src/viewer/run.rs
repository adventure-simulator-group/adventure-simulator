#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    archetype: BuildingArchetype,
    view: ViewerView,
    seed: u64,
    output: Option<PathBuf>,
    settle_frames: u32,
    projected_kind: ProjectedProofKind,
    roof_proof: Option<RoofProofView>,
    editor: bool,
    document_path: Option<PathBuf>,
    player_build_document_path: Option<PathBuf>,
) {
    let seed = if view == ViewerView::ArtilleryBridgeDenied {
        702
    } else if projected_view(view) {
        match (projected_kind, view) {
            (ProjectedProofKind::Breteche, _) => 201,
            (ProjectedProofKind::Hoarding, ViewerView::ProjectedSockets) => 42,
            (ProjectedProofKind::Hoarding, _) => 202,
            (ProjectedProofKind::Bartizan, _) => 203,
            (ProjectedProofKind::Machicolation, _) => 42,
        }
    } else {
        seed
    };
    let editor_document_path =
        document_path.unwrap_or_else(|| PathBuf::from("building-document.json"));
    let player_build_document = player_build_document_path.as_ref().map(|path| {
        let bytes = fs::read(path).unwrap_or_else(|error| {
            panic!(
                "failed to read player-build document {}: {error}",
                path.display()
            )
        });
        serde_json::from_slice::<PlayerBuildDocument>(&bytes).unwrap_or_else(|error| {
            panic!(
                "failed to decode player-build document {}: {error}",
                path.display()
            )
        })
    });
    let mut document = if editor && editor_document_path.exists() {
        let bytes = fs::read(&editor_document_path).unwrap_or_else(|error| {
            panic!(
                "failed to read editor document {}: {error}",
                editor_document_path.display()
            )
        });
        serde_json::from_slice::<BuildingDocument>(&bytes).unwrap_or_else(|error| {
            panic!(
                "failed to decode editor document {}: {error}",
                editor_document_path.display()
            )
        })
    } else {
        BuildingDocument::fixture(archetype, seed)
    };
    let mut program = document.program.clone();
    if let Some(proof) = roof_proof {
        program.roof_pitch_degrees = match proof {
            RoofProofView::RoofGableLowPitch => 25.0,
            RoofProofView::RoofGableMidPitch => 45.0,
            RoofProofView::RoofGableHighPitch => 70.0,
            _ => program.roof_pitch_degrees,
        };
        if matches!(
            proof,
            RoofProofView::RoofGableLowPitch
                | RoofProofView::RoofGableMidPitch
                | RoofProofView::RoofGableHighPitch
        ) {
            program.roof_demonstrator = Some(RoofKind::Gable);
        }
        if matches!(
            proof,
            RoofProofView::RoofRoundTowerExterior
                | RoofProofView::RoofRoundTowerTop
                | RoofProofView::RoofRoundTowerCutaway
                | RoofProofView::RoofRoundTowerDrainage
        ) {
            program.roof_demonstrator = Some(RoofKind::Conical);
        }
        if matches!(
            proof,
            RoofProofView::RoofPavilionExterior
                | RoofProofView::RoofPavilionTop
                | RoofProofView::RoofPavilionCutaway
                | RoofProofView::RoofPavilionDrainage
        ) {
            program.roof_demonstrator = Some(RoofKind::Pavilion);
        }
    }
    document.program = program.clone();
    let plan = if editor {
        generate_document(&document).expect("editor document must generate")
    } else {
        generate(&program).expect("curated building fixture must generate")
    };
    let plan_bytes = serde_json::to_vec(&plan).expect("serialize building plan for evidence hash");
    let plan_hash = stable_evidence_hash(&plan_bytes);
    let evidence_hash = stable_evidence_hash(
        format!(
            "{plan_hash}|{}|{:?}|{:?}|{:?}|{seed}|{VIEW_WIDTH}x{VIEW_HEIGHT}",
            archetype.slug(),
            view,
            projected_kind,
            roof_proof,
        )
        .as_bytes(),
    );
    let resolved_geometry_hash = stable_evidence_hash(
        &serde_json::to_vec(&plan.resolved_geometry).expect("serialize resolved geometry"),
    );
    let roof_graph_hash = stable_evidence_hash(
        &serde_json::to_vec(&plan.roof_assemblies).expect("serialize resolved roof graph"),
    );
    let church_program_hash = plan.church.as_ref().map_or_else(String::new, |church| {
        stable_evidence_hash(&serde_json::to_vec(church).expect("serialize church assembly"))
    });
    let church_bay_labels = plan.church.as_ref().map_or_else(Vec::new, |church| {
        let mut labels = church
            .bay_assemblies
            .iter()
            .map(|bay| format!("N{}@{:.2}", bay.axis_index + 1, bay.axis_metres))
            .collect::<Vec<_>>();
        labels.push(format!("X@{:.2}", church.crossing_axis_metres));
        labels.extend(
            church
                .choir_axes_metres
                .iter()
                .enumerate()
                .map(|(index, axis)| format!("Q{}@{axis:.2}", index + 1)),
        );
        labels.push(format!("A{}", church.program.apse_sides));
        labels
    });
    let church_support_node_ids = plan.church.as_ref().map_or_else(Vec::new, |_| {
        plan.resolved_geometry
            .structural_nodes
            .iter()
            .filter(|node| {
                matches!(
                    node.kind,
                    adventuresim_building_generator::StructuralNodeKind::ChurchPier
                        | adventuresim_building_generator::StructuralNodeKind::ChurchArcadeSpringing
                        | adventuresim_building_generator::StructuralNodeKind::ChurchVaultSpringing
                        | adventuresim_building_generator::StructuralNodeKind::ChurchCrossingPier
                        | adventuresim_building_generator::StructuralNodeKind::ChurchButtress
                        | adventuresim_building_generator::StructuralNodeKind::ChurchTowerStage
                        | adventuresim_building_generator::StructuralNodeKind::ChurchBellFrame
                )
            })
            .map(|node| node.id.0)
            .collect()
    });
    let church_opening_ids = plan.church.as_ref().map_or_else(Vec::new, |_| {
        plan.opening_assemblies
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
            .map(|opening| opening.id.0)
            .collect()
    });
    let church_focus_ids = church_focus_item_ids(&plan, view);
    let church_focus_set = church_focus_ids
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let church_focused_roles = plan
        .resolved_geometry
        .solids
        .iter()
        .filter(|solid| church_focus_set.contains(&solid.id.0))
        .map(|solid| format!("{:?}", solid.role))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let church_target_component_ids = church_target_component_ids(&plan, view);
    let church_required_roles = church_required_roles(view);
    let church_cut_plane = church_cut_plane(&plan, view);
    let church_removed_target_item_ids = if church_section_proof(view) {
        architectural_section_removed_item_ids(&plan, view)
            .into_iter()
            .filter(|id| church_focus_set.contains(id))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let timber_focus_ids = timber_focus_item_ids(&plan, view);
    let timber_focus_set = timber_focus_ids
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let timber_focused_roles = plan
        .resolved_geometry
        .solids
        .iter()
        .filter(|solid| timber_focus_set.contains(&solid.id.0))
        .map(|solid| format!("{:?}", solid.role))
        .chain(
            plan.resolved_geometry
                .surfaces
                .iter()
                .filter(|surface| timber_focus_set.contains(&surface.id.0))
                .map(|surface| format!("{:?}", surface.role)),
        )
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut timber_role_item_ids = std::collections::BTreeMap::<String, Vec<u64>>::new();
    for solid in plan
        .resolved_geometry
        .solids
        .iter()
        .filter(|solid| timber_focus_set.contains(&solid.id.0))
    {
        timber_role_item_ids
            .entry(format!("{:?}", solid.role))
            .or_default()
            .push(solid.id.0);
    }
    for surface in plan
        .resolved_geometry
        .surfaces
        .iter()
        .filter(|surface| timber_focus_set.contains(&surface.id.0))
    {
        timber_role_item_ids
            .entry(format!("{:?}", surface.role))
            .or_default()
            .push(surface.id.0);
    }
    let timber_required_roles = timber_required_roles(&plan, view);
    let timber_cut_plane = timber_cut_plane(&plan, view);
    let timber_removed_target_item_ids = if timber_section_proof(view) {
        architectural_section_removed_item_ids(&plan, view)
            .into_iter()
            .filter(|id| timber_focus_set.contains(id))
            .collect()
    } else {
        Vec::new()
    };
    let focused_roof_indices = roof_proof
        .map(|proof| roof_proof_assembly_indices(&plan, proof))
        .or_else(|| {
            (view == ViewerView::TimberGableRoofBearing).then(|| {
                plan.roof_assemblies
                    .iter()
                    .enumerate()
                    .filter(|(_, roof)| roof.parent.is_none())
                    .map(|(index, _)| index)
                    .collect()
            })
        })
        .unwrap_or_default();
    let mut section_removed_roof_item_ids = roof_proof
        .filter(|proof| roof_proof_sectioned(*proof))
        .map(|_| {
            focused_roof_indices
                .iter()
                .filter_map(|index| {
                    plan.roof_assemblies[*index]
                        .faces
                        .last()
                        .map(|face| face.id.0)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    section_removed_roof_item_ids.extend(church_section_removed_roof_item_ids(&plan, view));
    let expected_roof_render_items = plan
        .roof_assemblies
        .iter()
        .flat_map(|roof| {
            roof.faces
                .iter()
                .filter(|face| !section_removed_roof_item_ids.contains(&face.id.0))
                .map(|face| {
                    (
                        face.id.0,
                        stable_u64(&serde_json::to_vec(face).expect("serialize roof face")),
                    )
                })
                .chain(roof.enclosure_faces.iter().map(|enclosure| {
                    (
                        enclosure.id.0,
                        stable_u64(
                            &serde_json::to_vec(enclosure).expect("serialize roof enclosure"),
                        ),
                    )
                }))
        })
        .collect::<Vec<_>>();
    let mut expected_roof_render_items = expected_roof_render_items;
    if timber_isolated_view(view) && view != ViewerView::TimberGableRoofBearing {
        expected_roof_render_items.clear();
    }
    let roof_render_multiset_hash =
        resolved_item_multiset_hash(expected_roof_render_items.iter().copied());
    let expected_render_hash = resolved_item_multiset_hash(
        plan.resolved_geometry
            .solids
            .iter()
            .filter(|solid| !timber_isolated_view(view) || timber_focus_set.contains(&solid.id.0))
            .filter(|solid| !timber_removed_target_item_ids.contains(&solid.id.0))
            .map(|solid| {
                (
                    solid.id.0,
                    stable_u64(&serde_json::to_vec(solid).expect("serialize resolved solid")),
                )
            })
            .chain(
                plan.resolved_geometry
                    .surfaces
                    .iter()
                    .filter(|surface| {
                        timber_isolated_view(view)
                            && timber_focus_set.contains(&surface.id.0)
                            && surface.role
                                == adventuresim_building_generator::SurfaceRole::TimberCirculation
                    })
                    .map(|surface| {
                        (
                            surface.id.0,
                            stable_u64(
                                &serde_json::to_vec(surface)
                                    .expect("serialize resolved timber route"),
                            ),
                        )
                    }),
            ),
    );
    let projected_focus = projected_view(view)
        .then(|| focused_projected_defense(&plan, view, projected_kind))
        .flatten();
    let architectural_owner = architectural_focus_owner(&plan, view);
    let architectural_items = architectural_focus_item_ids(&plan, view);
    let architectural_voids = architectural_focus_void_ids(&plan, view);
    let focused_roof_owners = focused_roof_indices
        .iter()
        .map(|index| plan.roof_assemblies[*index].owner)
        .collect::<std::collections::HashSet<_>>();
    let dormer_focus_roof = roof_proof
        .filter(|proof| roof_proof_slug(*proof).starts_with("roof-dormer-"))
        .and_then(|_| {
            plan.roof_assemblies
                .iter()
                .find(|roof| roof.parent.is_some())
        });
    let focused_roof_downspouts = plan
        .resolved_geometry
        .roof_drainage_networks
        .iter()
        .filter(|network| focused_roof_owners.contains(&network.owner))
        .filter_map(|network| network.downspout.map(|id| id.0))
        .collect::<std::collections::HashSet<_>>();
    let focused_abutment_item_ids = roof_proof
        .filter(|proof| roof_proof_slug(*proof).starts_with("roof-abutment-"))
        .map(|proof| {
            let wanted = if roof_proof_slug(proof).starts_with("roof-abutment-tower-") {
                adventuresim_building_generator::RoofAbutmentKind::Tower
            } else {
                adventuresim_building_generator::RoofAbutmentKind::Wall
            };
            focused_roof_indices
                .iter()
                .flat_map(|index| &plan.roof_assemblies[*index].abutments)
                .filter(|abutment| abutment.kind == wanted)
                .flat_map(|abutment| {
                    abutment.samples.iter().flat_map(|sample| {
                        [
                            sample.apron_solid.0,
                            sample.upstand_solid.0,
                            sample.counterflashing_solid.0,
                        ]
                    })
                })
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    let mut focused_roof_item_ids = focused_roof_indices
        .iter()
        .flat_map(|index| {
            let roof = &plan.roof_assemblies[*index];
            roof.faces
                .iter()
                .map(|face| face.id.0)
                .chain(roof.enclosure_faces.iter().map(|face| face.id.0))
        })
        .collect::<Vec<_>>();
    if let Some(dormer) = dormer_focus_roof {
        focused_roof_item_ids = dormer
            .faces
            .iter()
            .map(|face| face.id.0)
            .chain(dormer.enclosure_faces.iter().map(|face| face.id.0))
            .collect();
    }
    if view == ViewerView::Exterior && roof_proof.is_none() {
        focused_roof_item_ids = plan
            .roof_assemblies
            .iter()
            .flat_map(|roof| {
                roof.faces
                    .iter()
                    .map(|face| face.id.0)
                    .chain(roof.enclosure_faces.iter().map(|face| face.id.0))
            })
            .collect();
    }
    let architectural_focus_hash = architectural_owner.map(|owner| {
        let solids = plan
            .resolved_geometry
            .solids
            .iter()
            .filter(|solid| solid.owner.0 == owner)
            .collect::<Vec<_>>();
        let voids = plan
            .resolved_geometry
            .voids
            .iter()
            .filter(|void| void.owner.0 == owner)
            .collect::<Vec<_>>();
        let surfaces = plan
            .resolved_geometry
            .surfaces
            .iter()
            .filter(|surface| surface.owner.0 == owner)
            .collect::<Vec<_>>();
        stable_evidence_hash(
            &serde_json::to_vec(&(solids, surfaces, voids)).expect("serialize focused geometry"),
        )
    });
    let section_annotation = if timber_proof_suffix(view).is_some() {
        plan.timber_frame.as_ref().map_or_else(String::new, |frame| {
            let detail = if view == ViewerView::TimberJointClose {
                frame
                    .joints
                    .iter()
                    .find(|joint| {
                        joint.member_ids.iter().all(|member_id| {
                            frame.members.iter().any(|member| {
                                member.id == *member_id
                                    && timber_focus_set.contains(&member.solid.0)
                            })
                        }) && joint.member_ids.len() >= 2
                    })
                    .map(|joint| {
                        format!(
                            " joint={} kind={:?} participants={:?} contacts={:?}",
                            joint.id.0,
                            joint.kind,
                            joint.member_ids,
                            joint.contact_interfaces
                        )
                    })
                    .unwrap_or_default()
            } else if matches!(
                view,
                ViewerView::TimberOpeningBayExterior
                    | ViewerView::TimberOpeningBayInterior
                    | ViewerView::TimberOpeningBaySection
            ) {
                format!(
                    " panels={:?} voids={:?}",
                    timber_role_item_ids.get("WallHost").cloned().unwrap_or_default(),
                    plan.timber_frame
                        .as_ref()
                        .into_iter()
                        .flat_map(|frame| &frame.bays)
                        .find_map(|bay| bay.opening)
                )
            } else if view == ViewerView::TimberGableRoofBearing {
                format!(
                    " roof_faces={:?} bearing_interfaces={:?}",
                    focused_roof_item_ids, frame.roof_bearing_interfaces
                )
            } else {
                String::new()
            };
            format!(
                "timber={} program={:?} target={} members={} joints={} cut={:?} legend=sill/post/plate/brace/jetty/roof-bearing{}",
                frame.id.0,
                frame.program,
                timber_proof_slug(&plan, view).unwrap_or_default(),
                timber_focus_ids.len(),
                frame.joints.len(),
                timber_cut_plane,
                detail,
            )
        })
    } else if church_proof_slug(view).is_some() {
        plan.church.as_ref().map_or_else(String::new, |church| {
            format!(
                "target={:?} church={} type=cruciform_3_aisled_basilica bays=N1,N2,N3,N4,X,Q1,Q2,A5 roles={:?} cut={:?} openings={} supports={} public_route=[{},{},{}] route=1.80x2.95m datum_floor={:.2}m vault={:.2}m",
                church_target_component_ids,
                church.id.0,
                church_required_roles,
                church_cut_plane,
                church_opening_ids.len(),
                church_support_node_ids.len(),
                church.tower.exterior_approach_surface.0,
                church.tower.vestibule_surface.0,
                church.tower.nave_entry_surface.0,
                church.datum.floor_metres,
                church.datum.vault_crown_metres,
            )
        })
    } else if artillery_proof_slug(view).is_some() {
        plan.artillery_castle.as_ref().map_or_else(String::new, |castle| {
            format!(
                "artillery={} phase={:?} target={} curtains={:?} rondels={:?} stations={} routes={} fire_rays={} cut={:?} legend=fieldstone/earth/timber/inside/outside",
                castle.id.0,
                castle.phase,
                artillery_proof_slug(view).unwrap_or_default(),
                castle.curtains.iter().map(|curtain| curtain.id.0).collect::<Vec<_>>(),
                castle.rondels.iter().map(|rondel| rondel.id.0).collect::<Vec<_>>(),
                castle.stations.len(),
                castle.route_edges.len(),
                castle.stations.iter().map(|station| station.rays.len()).sum::<usize>(),
                artillery_cut_plane(view),
            )
        })
    } else if section_proof(view) {
        if let Some(opening) = focused_opening(&plan, view) {
            let wall = plan
                .wall_assemblies
                .iter()
                .find(|wall| wall.id == opening.host_wall)
                .expect("focused opening wall");
            format!(
                "wall={} opening={} profile={} thickness={:.2}m throat={:.2}m mouth={:.2}m",
                wall.id.0,
                opening.id.0,
                opening_profile_slug(opening.profile),
                wall.thickness_metres,
                opening.profile.exterior_width_metres(),
                opening.profile.interior_width_metres(),
            )
        } else if let Some(wall) = focused_wall(&plan, view) {
            format!(
                "wall={} opening=none profile=solid_section thickness={:.2}m",
                wall.id.0, wall.thickness_metres
            )
        } else {
            format!(
                "wall=round_tower opening=radial profile=shell_section thickness={:.2}m",
                plan.towers
                    .first()
                    .map_or(0.0, |tower| tower.wall_thickness_metres)
            )
        }
    } else if let Some(proof) = roof_proof {
        format!(
            "roof_view={} assemblies={:?} graph_hash={}",
            roof_proof_slug(proof),
            focused_roof_indices
                .iter()
                .map(|index| plan.roof_assemblies[*index].id.0)
                .collect::<Vec<_>>(),
            roof_graph_hash,
        )
    } else {
        String::new()
    };
    if let Some(path) = &output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create building capture directory");
        }
        fs::write(
            path.with_extension("plan.json"),
            serde_json::to_vec_pretty(&plan).expect("serialize building plan"),
        )
        .expect("write generated building plan");
    }

    let plan_audit = audit_plan(&plan);
    for issue in &plan_audit {
        eprintln!("plan audit {}: {}", issue.code, issue.message);
    }
    let manifest = CaptureManifest {
        schema_version: 1,
        fixture: archetype.slug(),
        view: if let Some(roof_view) = roof_proof {
            roof_proof_slug(roof_view)
        } else if let Some(timber_view) = timber_proof_suffix(view) {
            timber_view
        } else if let Some(church_view) = church_proof_slug(view) {
            church_view
        } else if let Some(artillery_view) = artillery_proof_slug(view) {
            artillery_view
        } else {
            match view {
                ViewerView::Exterior => "exterior",
                ViewerView::Defenses => "defenses",
                ViewerView::Cutaway => "cutaway",
                ViewerView::GateDetailExterior => "gate-detail-exterior",
                ViewerView::GateDetailInterior => "gate-detail-interior",
                ViewerView::TowerPortalDetail => "tower-portal-detail",
                ViewerView::CrownStraightExterior => "crown-straight-exterior",
                ViewerView::CrownStraightInterior => "crown-straight-interior",
                ViewerView::CrownCornerExterior => "crown-corner-exterior",
                ViewerView::CrownCornerInterior => "crown-corner-interior",
                ViewerView::CrownTowerExterior => "crown-tower-exterior",
                ViewerView::CrownTowerTop => "crown-tower-top",
                ViewerView::CrownTowerCutaway => "crown-tower-cutaway",
                ViewerView::ProjectedExterior => "projected-exterior",
                ViewerView::ProjectedInterior => "projected-interior",
                ViewerView::ProjectedUnderside => "projected-underside",
                ViewerView::ProjectedTop => "projected-top",
                ViewerView::ProjectedLongitudinal => "projected-longitudinal",
                ViewerView::ProjectedSockets => "projected-sockets",
                ViewerView::ProjectedFlank => "projected-flank",
                ViewerView::OpeningRectangularExterior => "opening-rectangular-exterior",
                ViewerView::OpeningRectangularInterior => "opening-rectangular-interior",
                ViewerView::OpeningRectangularSection => "opening-rectangular-section",
                ViewerView::OpeningSegmentalExterior => "opening-segmental-exterior",
                ViewerView::OpeningSegmentalInterior => "opening-segmental-interior",
                ViewerView::OpeningSegmentalSection => "opening-segmental-section",
                ViewerView::OpeningPointedExterior => "opening-pointed-exterior",
                ViewerView::OpeningPointedInterior => "opening-pointed-interior",
                ViewerView::OpeningPointedSection => "opening-pointed-section",
                ViewerView::OpeningArrowLoopExterior => "opening-arrow-loop-exterior",
                ViewerView::OpeningArrowLoopInterior => "opening-arrow-loop-interior",
                ViewerView::OpeningArrowLoopSection => "opening-arrow-loop-section",
                ViewerView::OpeningGunLoopExterior => "opening-gun-loop-exterior",
                ViewerView::OpeningGunLoopInterior => "opening-gun-loop-interior",
                ViewerView::OpeningGunLoopSection => "opening-gun-loop-section",
                ViewerView::WallTimberFrameSection => "wall-timber-frame-section",
                ViewerView::WallCivilianMasonrySection => "wall-civilian-masonry-section",
                ViewerView::WallCathedralButtressSection => "wall-cathedral-buttress-section",
                ViewerView::WallRoundTowerRadialSection => "wall-round-tower-radial-section",
                _ => unreachable!("church views handled before ordinary view mapping"),
            }
        },
        seed,
        resolution: [VIEW_WIDTH, VIEW_HEIGHT],
        room_count: plan.storeys.iter().map(|storey| storey.rooms.len()).sum(),
        wall_count: plan.storeys.iter().map(|storey| storey.walls.len()).sum(),
        opening_count: plan
            .storeys
            .iter()
            .map(|storey| storey.openings.len())
            .sum(),
        roof_piece_count: plan.roofs.len(),
        roof_dormer_count: plan.roof_dormers.len(),
        roof_assembly_count: plan.roof_assemblies.len(),
        roof_graph_hash,
        roof_face_ids: plan
            .roof_assemblies
            .iter()
            .flat_map(|roof| roof.faces.iter().map(|face| face.id.0))
            .collect(),
        roof_edge_ids: plan
            .roof_assemblies
            .iter()
            .flat_map(|roof| roof.edges.iter().map(|edge| edge.id.0))
            .collect(),
        roof_cut_ids: plan
            .roof_assemblies
            .iter()
            .flat_map(|roof| roof.children.iter().map(|child| child.parent_cut.0))
            .collect(),
        roof_support_node_ids: plan
            .roof_assemblies
            .iter()
            .flat_map(|roof| roof.support_nodes.iter().map(|node| node.0))
            .collect(),
        roof_drainage_terminal_ids: plan
            .roof_assemblies
            .iter()
            .flat_map(|roof| {
                roof.edges
                    .iter()
                    .filter_map(|edge| edge.drainage_terminal.map(|terminal| terminal.0))
            })
            .collect(),
        roof_drainage_network_ids: plan
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .filter(|network| focused_roof_owners.contains(&network.owner))
            .map(|network| network.id.0)
            .collect(),
        roof_drainage_channel_ids: plan
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .filter(|network| focused_roof_owners.contains(&network.owner))
            .flat_map(|network| {
                std::iter::once(network.channel_floor.0)
                    .chain(network.channel_lips.iter().map(|id| id.0))
                    .chain(network.collector_solids.iter().map(|id| id.0))
                    .chain(network.downspout.iter().map(|id| id.0))
            })
            .collect(),
        roof_drainage_outlet_ids: plan
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .filter(|network| focused_roof_owners.contains(&network.owner))
            .map(|network| network.outlet_void.0)
            .collect(),
        roof_drainage_route_ids: plan
            .resolved_geometry
            .roof_drainage_networks
            .iter()
            .filter(|network| focused_roof_owners.contains(&network.owner))
            .filter_map(|network| {
                plan.resolved_geometry
                    .drainage_catchments
                    .iter()
                    .find(|catchment| catchment.id == network.catchment)
                    .map(|catchment| catchment.outlet_route.0)
            })
            .collect(),
        roof_render_item_count: expected_roof_render_items.len(),
        roof_render_multiset_hash,
        rendered_roof_item_count: 0,
        rendered_roof_hash: String::new(),
        tower_count: plan.towers.len(),
        square_tower_count: plan.square_towers.len(),
        curtain_wall_count: plan.curtain_walls.len(),
        stair_count: plan.stairs.len(),
        battlement_run_count: plan.battlements.len(),
        wall_walk_count: plan.wall_walks.len(),
        defensive_circuit_count: plan.defensive_circuits.len(),
        defensive_junction_count: plan.defensive_junctions.len(),
        tower_portal_count: plan.tower_portals.len(),
        gate_defense_count: plan.gate_defenses.len(),
        firing_position_count: plan
            .gate_defenses
            .iter()
            .map(|defense| defense.firing_positions.len())
            .sum(),
        gate_closure_count: plan
            .gate_defenses
            .iter()
            .map(|defense| defense.closures.len())
            .sum(),
        resolved_solid_count: plan.resolved_geometry.solids.len(),
        resolved_void_count: plan.resolved_geometry.voids.len(),
        resolved_owner_count: plan
            .resolved_geometry
            .solids
            .iter()
            .map(|solid| solid.owner)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        rendered_owner_count: 0,
        rendered_resolved_solid_count: 0,
        resolver_schema_version: plan.resolved_geometry.schema_version,
        resolved_geometry_hash,
        resolved_solid_multiset_hash: expected_render_hash,
        rendered_geometry_hash: String::new(),
        source_revision: source_revision(),
        source_dirty_fingerprint: source_dirty_fingerprint(),
        plan_hash,
        evidence_hash,
        pixel_hash: String::new(),
        focus_kind: if roof_proof.is_some() {
            Some("resolved_roof")
        } else if timber_proof_suffix(view).is_some() {
            Some("resolved_timber_frame")
        } else if church_proof_slug(view).is_some() {
            Some("resolved_church_program")
        } else {
            match view {
                ViewerView::GateDetailExterior => Some("gate_exterior"),
                ViewerView::GateDetailInterior => Some("gate_interior_section"),
                ViewerView::TowerPortalDetail => Some("tower_portal"),
                ViewerView::CrownStraightExterior
                | ViewerView::CrownStraightInterior
                | ViewerView::CrownCornerExterior
                | ViewerView::CrownCornerInterior
                | ViewerView::CrownTowerExterior
                | ViewerView::CrownTowerTop
                | ViewerView::CrownTowerCutaway => Some("resolved_crown"),
                ViewerView::ProjectedExterior
                | ViewerView::ProjectedInterior
                | ViewerView::ProjectedUnderside
                | ViewerView::ProjectedTop
                | ViewerView::ProjectedLongitudinal
                | ViewerView::ProjectedSockets
                | ViewerView::ProjectedFlank => Some("resolved_projected"),
                view if opening_proof_profile(view).is_some() => Some("resolved_opening"),
                view if wall_section_kind(view).is_some() => Some("resolved_wall_section"),
                view if artillery_proof_slug(view).is_some() => Some("artillery_assembly"),
                _ => None,
            }
        },
        focused_tower_index: (view == ViewerView::TowerPortalDetail).then_some(0),
        focused_tower_indices: match view {
            ViewerView::GateDetailExterior => plan
                .gate_defenses
                .first()
                .map(|defense| {
                    defense
                        .firing_positions
                        .iter()
                        .map(|position| position.tower_index)
                        .collect()
                })
                .unwrap_or_default(),
            ViewerView::TowerPortalDetail => vec![0],
            _ => Vec::new(),
        },
        focused_wall_index: matches!(
            view,
            ViewerView::GateDetailExterior | ViewerView::GateDetailInterior
        )
        .then_some(0),
        focused_resolved_item_ids: if roof_proof.is_some() {
            plan.resolved_geometry
                .solids
                .iter()
                .filter(|solid| {
                    if focused_abutment_item_ids.is_empty() {
                        dormer_focus_roof.map_or_else(
                            || focused_roof_owners.contains(&solid.owner),
                            |dormer| solid.owner == dormer.owner,
                        ) && (roof_proof
                            .is_some_and(|proof| roof_proof_slug(proof).ends_with("-drainage"))
                            || !focused_roof_downspouts.contains(&solid.id.0))
                    } else {
                        focused_abutment_item_ids.contains(&solid.id.0)
                    }
                })
                .map(|solid| solid.id.0)
                .collect()
        } else if timber_proof_suffix(view).is_some() {
            timber_focus_ids.clone()
        } else if church_proof_slug(view).is_some() {
            church_focus_ids.clone()
        } else if artillery_proof_slug(view).is_some() {
            artillery_focus_item_ids(&plan, view)
        } else if architectural_proof(view) {
            architectural_items.clone()
        } else if projected_view(view) {
            focused_projected_item_ids(&plan, view, projected_kind)
        } else if view == ViewerView::Exterior {
            plan.resolved_geometry
                .solids
                .iter()
                .map(|solid| solid.id.0)
                .collect()
        } else {
            focused_crown_item_ids(&plan, view)
        },
        focused_resolved_void_ids: if roof_proof
            .is_some_and(|proof| roof_proof_slug(proof).ends_with("-drainage"))
        {
            plan.resolved_geometry
                .roof_drainage_networks
                .iter()
                .filter(|network| focused_roof_owners.contains(&network.owner))
                .map(|network| network.outlet_void.0)
                .collect()
        } else if artillery_proof_slug(view).is_some() {
            artillery_focus_void_ids(&plan, view)
        } else if architectural_proof(view) {
            architectural_voids.clone()
        } else if matches!(
            view,
            ViewerView::TimberOpeningBayExterior
                | ViewerView::TimberOpeningBayInterior
                | ViewerView::TimberOpeningBaySection
        ) {
            plan.timber_frame
                .as_ref()
                .into_iter()
                .flat_map(|frame| &frame.bays)
                .find_map(|bay| bay.opening)
                .and_then(|opening_id| {
                    plan.opening_assemblies
                        .iter()
                        .find(|opening| opening.id == opening_id)
                })
                .map(|opening| vec![opening.void_id.0])
                .unwrap_or_default()
        } else {
            projected_focus
                .map(|defense| {
                    defense
                        .throat_voids
                        .iter()
                        .copied()
                        .chain(defense.access_portal)
                        .chain(defense.firing_apertures.iter().copied())
                        .map(|id| id.0)
                        .collect()
                })
                .unwrap_or_default()
        },
        focused_roof_item_ids,
        section_removed_roof_item_ids,
        visible_focused_roof_item_count: 0,
        focused_projected_ray_count: projected_focus
            .map(|defense| {
                plan.resolved_geometry
                    .projected_defense_rays
                    .iter()
                    .filter(|ray| ray.owner == defense.owner)
                    .count()
            })
            .unwrap_or(0),
        projected_defense_kind: projected_focus.map(|_| projected_kind_slug(projected_kind)),
        projected_defense_deployment: projected_focus
            .map(|defense| projected_deployment_slug(defense.deployment)),
        projected_tactical_target: projected_focus
            .map(|defense| projected_target_slug(defense.tactical_target)),
        visible_focused_resolved_item_count: 0,
        focused_bounds_fraction: [0.0; 4],
        camera_position: [0.0; 3],
        camera_target: [0.0; 3],
        required_focus_object_count: if roof_proof.is_some() {
            focused_roof_indices.len().max(1)
        } else if timber_proof_suffix(view).is_some() {
            timber_focus_ids
                .len()
                .saturating_sub(timber_removed_target_item_ids.len())
                .clamp(2, 8)
        } else if church_proof_slug(view).is_some() {
            8
        } else {
            match view {
                ViewerView::GateDetailExterior => 6,
                ViewerView::GateDetailInterior => 11,
                ViewerView::TowerPortalDetail => 6,
                ViewerView::CrownStraightExterior
                | ViewerView::CrownStraightInterior
                | ViewerView::CrownCornerExterior
                | ViewerView::CrownCornerInterior
                | ViewerView::CrownTowerExterior
                | ViewerView::CrownTowerTop
                | ViewerView::CrownTowerCutaway => 4,
                ViewerView::ProjectedExterior
                | ViewerView::ProjectedInterior
                | ViewerView::ProjectedUnderside
                | ViewerView::ProjectedTop
                | ViewerView::ProjectedLongitudinal
                | ViewerView::ProjectedSockets
                | ViewerView::ProjectedFlank => 3,
                view if opening_proof_profile(view).is_some() => 3,
                view if wall_section_kind(view).is_some() => 1,
                _ => 0,
            }
        },
        visible_focus_object_count: 0,
        focus_requirements_met: false,
        lighting_preset: match view {
            ViewerView::GateDetailInterior => "clear_working_daylight_section_high_sun",
            ViewerView::GateDetailExterior => "clear_working_daylight_detail_framed",
            ViewerView::TowerPortalDetail => "clear_working_daylight_detail_high_sun",
            ViewerView::CrownStraightExterior
            | ViewerView::CrownStraightInterior
            | ViewerView::CrownCornerExterior
            | ViewerView::CrownCornerInterior
            | ViewerView::CrownTowerExterior
            | ViewerView::CrownTowerTop
            | ViewerView::CrownTowerCutaway => "clear_working_daylight_crown_proof",
            ViewerView::ProjectedExterior
            | ViewerView::ProjectedInterior
            | ViewerView::ProjectedUnderside
            | ViewerView::ProjectedTop
            | ViewerView::ProjectedLongitudinal
            | ViewerView::ProjectedSockets
            | ViewerView::ProjectedFlank => "clear_working_daylight_projected_defense_proof",
            _ => "clear_working_daylight",
        },
        sun_direction: if let Some(defense) = projected_focus {
            let outward = match defense.path {
                ProjectedDefensePath::Linear { outward, .. }
                | ProjectedDefensePath::Round { outward, .. } => direction_vector_2d(outward),
            };
            let tangent = Vec2::new(-outward.y, outward.x);
            let (outward_scale, tangent_scale) = if matches!(
                view,
                ViewerView::ProjectedLongitudinal | ViewerView::ProjectedTop
            ) {
                (34.0, 18.0)
            } else {
                (18.0, 34.0)
            };
            (-Vec3::new(
                outward.x * outward_scale + tangent.x * tangent_scale,
                45.0,
                outward.y * outward_scale + tangent.y * tangent_scale,
            )
            .normalize())
            .to_array()
        } else if roof_proof
            .is_some_and(|proof| roof_proof_slug(proof) == "roof-cross-gable-exterior")
        {
            [0.55, -0.75, 0.39]
        } else {
            match view {
                ViewerView::GateDetailInterior => [-0.42, -0.75, -0.51],
                ViewerView::GateDetailExterior => [-0.45, -0.61, 0.55],
                ViewerView::TowerPortalDetail => [-0.42, -0.75, 0.51],
                ViewerView::CrownStraightInterior => [0.45, -0.72, -0.55],
                ViewerView::CrownCornerInterior => [-0.45, -0.72, -0.55],
                ViewerView::CrownStraightExterior
                | ViewerView::CrownCornerExterior
                | ViewerView::CrownTowerExterior
                | ViewerView::CrownTowerTop
                | ViewerView::CrownTowerCutaway => [-0.5, -0.72, 0.48],
                ViewerView::Defenses => [0.62, -0.69, -0.36],
                ViewerView::TimberFrameFacade => [-0.64, -0.75, 0.15],
                ViewerView::TimberGableRoofBearing => [-0.67, -0.72, 0.18],
                _ => [-0.45, -0.61, 0.55],
            }
        },
        sun_illuminance_lux: if projected_focus.is_some() {
            20_000.0
        } else if matches!(
            view,
            ViewerView::CrownStraightExterior
                | ViewerView::CrownStraightInterior
                | ViewerView::CrownCornerExterior
                | ViewerView::CrownCornerInterior
                | ViewerView::CrownTowerExterior
                | ViewerView::CrownTowerTop
                | ViewerView::CrownTowerCutaway
        ) || timber_proof_suffix(view).is_some()
        {
            28_000.0
        } else {
            24_000.0
        },
        ambient_brightness: if view == ViewerView::ProjectedInterior {
            420.0
        } else if view == ViewerView::ProjectedUnderside {
            380.0
        } else if roof_proof
            .is_some_and(|proof| roof_proof_slug(proof) == "roof-cross-gable-exterior")
        {
            320.0
        } else if roof_proof.is_some_and(|proof| roof_proof_slug(proof).ends_with("-interior")) {
            400.0
        } else if roof_proof.is_some() {
            240.0
        } else if matches!(
            view,
            ViewerView::CrownStraightExterior
                | ViewerView::CrownStraightInterior
                | ViewerView::CrownCornerExterior
                | ViewerView::CrownCornerInterior
                | ViewerView::CrownTowerExterior
                | ViewerView::CrownTowerTop
                | ViewerView::CrownTowerCutaway
        ) || projected_focus.is_some()
        {
            340.0
        } else if matches!(
            view,
            ViewerView::TimberRegistrationCut | ViewerView::TimberGableRoofBearing
        ) {
            175.0
        } else if timber_proof_suffix(view).is_some() {
            220.0
        } else {
            380.0
        },
        ambient_color: [0.72, 0.78, 0.88],
        lighting_calibration_bounds_fraction: [0.0; 4],
        median_luminance_percent: 0,
        dark_clipped_bps: 0,
        bright_clipped_bps: 0,
        luminance_separation_percent: 0,
        shadow_luminance_percent: 0,
        plan_audit_issue_count: plan_audit.len(),
        audited_closed_mesh_count: 0,
        mesh_integrity_issue_count: 0,
        bartizan_count: plan.bartizans.len(),
        observed_mesh_count: 0,
        visible_mesh_count: 0,
        active_camera_count: 0,
        subject_pixel_bps: 0,
        validation_passed: false,
        opening_profile: opening_proof_profile(view),
        wall_section_kind: wall_section_kind(view),
        focused_assembly_owner_id: architectural_owner,
        focused_resolved_geometry_hash: architectural_focus_hash,
        section_cut_applied: section_proof(view)
            || church_section_proof(view)
            || timber_section_proof(view)
            || artillery_section_proof(view)
            || roof_proof.is_some_and(roof_proof_sectioned),
        section_removed_item_ids: if section_proof(view)
            || church_section_proof(view)
            || timber_section_proof(view)
            || artillery_section_proof(view)
        {
            architectural_section_removed_item_ids(&plan, view)
                .into_iter()
                .filter(|id| {
                    (!church_section_proof(view) || church_focus_ids.contains(id))
                        && (!timber_section_proof(view) || timber_focus_ids.contains(id))
                })
                .collect()
        } else {
            Vec::new()
        },
        inside_label_visible: section_proof(view)
            || timber_section_proof(view)
            || artillery_section_proof(view),
        outside_label_visible: section_proof(view)
            || timber_section_proof(view)
            || artillery_section_proof(view),
        wall_thickness_metres: focused_opening(&plan, view)
            .and_then(|opening| {
                plan.wall_assemblies
                    .iter()
                    .find(|wall| wall.id == opening.host_wall)
            })
            .or_else(|| focused_wall(&plan, view))
            .map(|wall| wall.thickness_metres)
            .or_else(|| {
                (view == ViewerView::WallRoundTowerRadialSection)
                    .then(|| plan.towers.first().map(|tower| tower.wall_thickness_metres))
                    .flatten()
            }),
        scale_figure_height_metres: (section_proof(view)
            || church_section_proof(view)
            || timber_section_proof(view)
            || artillery_section_proof(view))
        .then_some(1.75),
        scale_figure_visible: section_proof(view)
            || church_section_proof(view)
            || timber_section_proof(view)
            || artillery_section_proof(view),
        section_annotation,
        section_annotation_visible: false,
        exterior_throat_bounds_fraction: [0.0; 4],
        interior_mouth_bounds_fraction: [0.0; 4],
        church_program_hash,
        church_bay_labels,
        church_support_node_ids,
        church_opening_ids,
        church_focused_roles,
        church_target_component_ids,
        church_target_item_ids: church_focus_ids.clone(),
        church_required_roles,
        church_cut_plane,
        church_removed_target_item_ids,
        church_legend_visible: false,
        timber_program_hash: plan
            .timber_frame
            .as_ref()
            .map_or_else(String::new, |frame| {
                stable_evidence_hash(&serde_json::to_vec(frame).expect("serialize timber frame"))
            }),
        timber_program: plan
            .timber_frame
            .as_ref()
            .map(|frame| format!("{:?}", frame.program)),
        timber_assembly_id: plan.timber_frame.as_ref().map(|frame| frame.id.0),
        timber_member_ids: plan.timber_frame.as_ref().map_or_else(Vec::new, |frame| {
            frame.members.iter().map(|member| member.id.0).collect()
        }),
        timber_joint_ids: plan.timber_frame.as_ref().map_or_else(Vec::new, |frame| {
            frame.joints.iter().map(|joint| joint.id.0).collect()
        }),
        timber_node_ids: plan.timber_frame.as_ref().map_or_else(Vec::new, |frame| {
            frame
                .members
                .iter()
                .flat_map(|member| [member.start_node.0, member.end_node.0])
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect()
        }),
        timber_focused_roles,
        timber_role_item_ids,
        timber_role_bounds_fraction: std::collections::BTreeMap::new(),
        timber_target_component_ids: timber_target_component_ids(&plan, view),
        timber_focus_interface_ids: timber_focus_interface_ids(&plan, view),
        timber_required_roles,
        timber_cut_plane,
        timber_removed_target_item_ids,
        timber_legend_visible: false,
        artillery_assembly_id: plan.artillery_castle.as_ref().map(|castle| castle.id.0),
        artillery_phase: plan
            .artillery_castle
            .as_ref()
            .map(|castle| format!("{:?}", castle.phase)),
        artillery_curtain_ids: plan
            .artillery_castle
            .as_ref()
            .map_or_else(Vec::new, |castle| {
                castle.curtains.iter().map(|curtain| curtain.id.0).collect()
            }),
        artillery_rondel_ids: plan
            .artillery_castle
            .as_ref()
            .map_or_else(Vec::new, |castle| {
                castle.rondels.iter().map(|rondel| rondel.id.0).collect()
            }),
        artillery_station_ids: plan
            .artillery_castle
            .as_ref()
            .map_or_else(Vec::new, |castle| {
                castle.stations.iter().map(|station| station.id.0).collect()
            }),
        artillery_route_surface_ids: plan.artillery_castle.as_ref().map_or_else(
            Vec::new,
            |castle| {
                castle
                    .route_nodes
                    .iter()
                    .map(|node| node.surface.0)
                    .collect()
            },
        ),
        artillery_fire_ray_count: plan.artillery_castle.as_ref().map_or(0, |castle| {
            castle
                .stations
                .iter()
                .map(|station| station.rays.len())
                .sum()
        }),
        artillery_support_node_ids: plan.artillery_castle.as_ref().map_or_else(
            Vec::new,
            |castle| {
                let owners = castle
                    .curtains
                    .iter()
                    .map(|curtain| curtain.owner)
                    .chain(castle.rondels.iter().map(|rondel| rondel.owner))
                    .collect::<std::collections::HashSet<_>>();
                plan.resolved_geometry
                    .structural_nodes
                    .iter()
                    .filter(|node| owners.contains(&node.owner))
                    .map(|node| node.id.0)
                    .collect()
            },
        ),
        artillery_ditch_void_id: plan
            .artillery_castle
            .as_ref()
            .map(|castle| castle.ditch.void_id.0),
        artillery_bridge_state: plan
            .artillery_castle
            .as_ref()
            .map(|castle| format!("{:?}", castle.bridge.state)),
        artillery_focused_roles: {
            let focus = artillery_focus_item_ids(&plan, view)
                .into_iter()
                .collect::<std::collections::HashSet<_>>();
            plan.resolved_geometry
                .solids
                .iter()
                .filter(|solid| focus.contains(&solid.id.0))
                .map(|solid| format!("{:?}", solid.role))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect()
        },
        artillery_role_item_ids: {
            let focus = artillery_focus_item_ids(&plan, view)
                .into_iter()
                .collect::<std::collections::HashSet<_>>();
            let mut roles = std::collections::BTreeMap::<String, Vec<u64>>::new();
            for solid in plan
                .resolved_geometry
                .solids
                .iter()
                .filter(|solid| focus.contains(&solid.id.0))
            {
                roles
                    .entry(format!("{:?}", solid.role))
                    .or_default()
                    .push(solid.id.0);
            }
            roles
        },
        artillery_role_bounds_fraction: std::collections::BTreeMap::new(),
        artillery_target_component_ids: artillery_proof_slug(view)
            .map(|slug| vec![format!("artillery:1/{slug}")])
            .unwrap_or_default(),
        artillery_cut_plane: artillery_cut_plane(view),
        artillery_removed_target_item_ids: artillery_section_removed_item_ids(&plan, view),
        artillery_legend_visible: false,
    };

    let title = format!("Fabelgeist building prototype: {archetype:?} {view:?}");
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title,
            resolution:
                WindowResolution::new(VIEW_WIDTH, VIEW_HEIGHT).with_scale_factor_override(1.0),
            present_mode: PresentMode::AutoNoVsync,
            resizable: false,
            decorations: output.is_none(),
            ..default()
        }),
        ..default()
    }))
    .insert_resource(ClearColor(Color::srgb(0.72, 0.80, 0.86)))
    .insert_resource(CaptureState {
        output,
        settle_frames,
        settled: 0,
        primed: false,
        in_flight: false,
        manifest,
    });
    if editor {
        app.add_plugins((
            MeshPickingPlugin,
            OutlinePlugin::JUMP_FLOOD,
            EguiPlugin::default(),
            PanOrbitCameraPlugin,
        ))
        .insert_resource(EditorRuntime::new(
            document,
            plan.clone(),
            editor_document_path,
            player_build_document.clone(),
            player_build_document_path,
        ))
        .add_observer(editor_pointer_over)
        .add_observer(editor_pointer_out)
        .add_observer(editor_pointer_click)
        .add_observer(editor_wall_drag_start)
        .add_observer(editor_wall_drag_move)
        .add_observer(editor_wall_drag_end)
        .add_systems(EguiPrimaryContextPass, editor_ui)
        .add_systems(
            Update,
            (
                update_editor_outlines,
                frame_editor_selection,
                editor_keyboard_shortcuts,
                update_editor_visibility,
                draw_wall_preview,
            ),
        )
        .add_systems(PostUpdate, rebuild_editor_scene);
    }
    let startup_plan = plan.clone();
    app.add_systems(Startup, move |world: &mut World| {
        setup(
            world,
            &startup_plan,
            view,
            projected_kind,
            roof_proof,
            if editor {
                SceneSetup::EditorInitial
            } else {
                SceneSetup::Full
            },
        );
        if editor {
            configure_editor_scene(world, &startup_plan, true);
        }
        if let Some(document) = &player_build_document {
            setup_player_build_scene(world, document);
        }
        if editor {
            world
                .run_system_once(update_editor_visibility)
                .expect("editor visibility system must run after initial scene setup");
        }
    })
    .add_systems(Last, capture_when_ready);
    let exit = app.run();
    if exit != AppExit::Success {
        std::process::exit(1);
    }
}
