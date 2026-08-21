use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use bevy::math::{Vec2, Vec3};
use geo::{BooleanOps, Coord, LineString, MultiPolygon, Polygon};
use thiserror::Error;

use crate::{
    AccessBrace, AccessDoor, AccessGuardSegment, AccessLanding, AccessLedger, AccessStairFlight,
    AuditIssue, BUILDING_DOCUMENT_SCHEMA_VERSION, Bartizan, BattlementKind, BattlementRun,
    BuildingArchetype, BuildingDocument, BuildingEdit, BuildingPlan, BuildingProgram,
    CELL_SIZE_METRES, CROWN_DRAIN_CHANNEL_WIDTH_METRES, Cell, CellDiameter, CrownAssembly,
    CrownJunction, CrownJunctionKind, CrownMaterial, CrownPath, CrownPattern, CrownPhase,
    CrownProfile, CurtainWallRun, DefenderSample, DefensiveCircuit, DefensiveJunction,
    DefensiveJunctionKind, Direction, DormerKind, DrainageCatchment, DrainageRoute, FiringPosition,
    Footprint, GRID_UNIT_METRES, GableProfile, GateClosure, GateClosureKind, GateDefense,
    GateGuardChamber, GateOperatingPosition, GatehouseAssemblySpec, GatehouseLoadPath,
    GeometryOwnerId, GridLength, GridPoint, GuardChamberAccess, GuardChamberOpening,
    GuardChamberSupport, GuardOpeningKind, InnerEdgeTreatment, JunctionBond, Opening, OpeningKind,
    ProjectedDefenseAssembly, ProjectedDefenseDeployment, ProjectedDefenseHostTopology,
    ProjectedDefenseHostWallSource, ProjectedDefenseKind, ProjectedDefenseMaterial,
    ProjectedDefensePath, ProjectedDefensePhase, ProjectedDefenseRange, ProjectedDefenseRay,
    ProjectedDefenseTarget, ProjectedDefenseWorkingPoint, ResolvedBounds, ResolvedGeometry,
    ResolvedItemId, ResolvedSolid, ResolvedSurface, ResolvedVoid, RidgeAxis, RoofAbutmentAssembly,
    RoofAbutmentKind, RoofAbutmentSample, RoofAssembly, RoofAssemblyId, RoofChildAssembly,
    RoofChildKind, RoofDormer, RoofDrainageDisposition, RoofDrainageNetwork,
    RoofDrainageOutletStation, RoofDrainageRecipient, RoofDrainageSample, RoofEdge, RoofEdgeKind,
    RoofEditError, RoofEnclosureFace, RoofFace, RoofFootprintLoop, RoofKind, RoofMaterial,
    RoofPhase, RoofPiece, RoofPivotPolicy, RoofPlaneEquation, Room, RoomKind, RoomRequirement,
    RoundTower, SolidRole, SquareTower, Stair, StoreyPlan, StructuralNode, StructuralNodeId,
    StructuralNodeKind, SupportInterface, SurfaceRole, TowerChordInterface, TowerPortal,
    TowerPortalKind, TraversalEnvelope, VoidRole, WallWalk,
};

fn grid_point(position: Vec2) -> GridPoint {
    let x = (position.x / GRID_UNIT_METRES).round() as i32;
    let z = (position.y / GRID_UNIT_METRES).round() as i32;
    debug_assert!((x as f32 * GRID_UNIT_METRES - position.x).abs() < 0.001);
    debug_assert!((z as f32 * GRID_UNIT_METRES - position.y).abs() < 0.001);
    GridPoint::new(x, z)
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum GenerationError {
    #[error("building footprint is empty or invalid")]
    InvalidFootprint,
    #[error("storey {level} has no requested rooms")]
    EmptyStorey { level: usize },
    #[error("storey {level} requests {rooms} rooms for only {cells} usable cells")]
    TooManyRooms {
        level: usize,
        rooms: usize,
        cells: usize,
    },
    #[error("storey {level} produced a disconnected room {room}")]
    DisconnectedRoom { level: usize, room: u16 },
    #[error("storey {level} does not have enough shared boundaries to connect its rooms")]
    DisconnectedStorey { level: usize },
    #[error("generated building failed the structural contract with {issues_count} audit issue(s)")]
    StructuralContract {
        issues_count: usize,
        issues: Vec<AuditIssue>,
    },
    #[error("building document schema {found} is unsupported; expected {expected}")]
    UnsupportedDocumentSchema { found: u32, expected: u32 },
    #[error("building edit target was not found: {0}")]
    EditTargetNotFound(String),
    #[error("building edit conflicts with existing authority: {0}")]
    EditConflict(String),
    #[error("building edit is not supported for this program: {0}")]
    UnsupportedEdit(String),
}

/// Dedicated projected-defense study tags change only the defense assembly,
/// not the host castle's room/circulation randomization. This keeps isolated
/// proofs comparable to the accepted seed-42 host instead of accidentally
/// introducing an unrelated disconnected layout.
fn layout_seed(program: &BuildingProgram) -> u64 {
    if program.archetype == BuildingArchetype::CastleGatehouse
        && matches!(program.seed % 1_000, 201..=203)
    {
        42
    } else {
        program.seed
    }
}

/// Generates a building that satisfies the complete structural contract.
///
/// `Ok` is a strong guarantee: the returned plan has passed [`crate::audit_plan`].
/// Programs that cannot produce a valid building are rejected with a typed error;
/// callers never receive a knowingly invalid plan.
pub fn generate(program: &BuildingProgram) -> Result<BuildingPlan, GenerationError> {
    let plan = generate_unchecked(program, &[])?;
    validate_generated_plan(plan)
}

/// Regenerates and audits a versioned editor document.
pub fn generate_document(document: &BuildingDocument) -> Result<BuildingPlan, GenerationError> {
    if document.schema_version != BUILDING_DOCUMENT_SCHEMA_VERSION {
        return Err(GenerationError::UnsupportedDocumentSchema {
            found: document.schema_version,
            expected: BUILDING_DOCUMENT_SCHEMA_VERSION,
        });
    }
    let mut program = document.program.clone();
    for edit in &document.edits {
        match *edit {
            BuildingEdit::SetWallStyle { style } => {
                if !matches!(
                    program.archetype,
                    BuildingArchetype::TownHouse
                        | BuildingArchetype::HallHouse
                        | BuildingArchetype::FachwerkCottage
                        | BuildingArchetype::FachwerkMerchantHouse
                        | BuildingArchetype::RenaissanceTownHall
                ) {
                    return Err(GenerationError::UnsupportedEdit(format!(
                        "{:?} has no editable civilian wall finish",
                        program.archetype
                    )));
                }
                program.wall_style = style;
            }
            BuildingEdit::SetTimberFrameStyle { style } => {
                if program.timber_frame_style.is_none() {
                    return Err(GenerationError::UnsupportedEdit(format!(
                        "{:?} has no timber-frame program",
                        program.archetype
                    )));
                }
                program.timber_frame_style = Some(style);
            }
            BuildingEdit::AddOpening { .. } | BuildingEdit::RemoveOpening { .. } => {}
        }
    }
    let plan = generate_unchecked(&program, &document.edits)?;
    validate_generated_plan(plan)
}

/// Applies one editor command transactionally. The returned document is only
/// produced when its regenerated plan passes the complete structural audit.
pub fn edit_document(
    document: &BuildingDocument,
    edit: BuildingEdit,
) -> Result<(BuildingDocument, BuildingPlan), GenerationError> {
    let mut candidate = document.clone();
    candidate.edits.push(edit);
    let plan = generate_document(&candidate)?;
    Ok((candidate, plan))
}

fn validate_generated_plan(plan: BuildingPlan) -> Result<BuildingPlan, GenerationError> {
    let issues = crate::audit_plan(&plan);
    if issues.is_empty() {
        Ok(plan)
    } else {
        Err(GenerationError::StructuralContract {
            issues_count: issues.len(),
            issues,
        })
    }
}

fn generate_unchecked(
    program: &BuildingProgram,
    edits: &[BuildingEdit],
) -> Result<BuildingPlan, GenerationError> {
    let footprint_cells = footprint_cells(program.footprint)?;
    let (width, depth) = program.footprint.dimensions();
    let mut storeys = Vec::with_capacity(program.storeys.len());
    let layout_seed = layout_seed(program);

    for (level, storey_program) in program.storeys.iter().enumerate() {
        if storey_program.rooms.is_empty() {
            return Err(GenerationError::EmptyStorey { level });
        }
        if storey_program.rooms.len() > footprint_cells.len() {
            return Err(GenerationError::TooManyRooms {
                level,
                rooms: storey_program.rooms.len(),
                cells: footprint_cells.len(),
            });
        }

        let assignments = allocate_rooms(
            &footprint_cells,
            width,
            depth,
            &storey_program.rooms,
            layout_seed.wrapping_add(level as u64 * 0x9e37_79b9),
            program.archetype,
        );
        let rooms = collect_rooms(&assignments, &storey_program.rooms);
        for room in &rooms {
            if !cells_are_connected(&room.cells) {
                return Err(GenerationError::DisconnectedRoom {
                    level,
                    room: room.id,
                });
            }
        }
        let walls = derive_walls(&footprint_cells, &assignments);
        let mut openings = derive_openings(
            &walls,
            &storey_program.rooms,
            program.archetype,
            layout_seed.wrapping_add(level as u64),
            level,
        )?;
        apply_opening_edits(storey_program, level as u16, &walls, &mut openings, edits)?;
        storeys.push(StoreyPlan {
            level: level as u16,
            rooms,
            walls,
            openings,
        });
    }
    if program.church_program.is_some() {
        // ChurchProgram is the sole wall/opening authority.  Rooms remain as
        // semantic occupancy labels, but the generic cell-wall vocabulary is
        // deliberately absent rather than hidden behind duplicate masonry.
        for storey in &mut storeys {
            storey.walls.clear();
            storey.openings.clear();
        }
    }

    let roofs = derive_roofs(program);
    let roof_dormers = derive_roof_dormers(program);
    let curtain_walls = derive_curtain_walls(program);
    let gatehouse_assemblies = derive_gatehouse_assemblies(program);
    let towers = derive_towers(program, &gatehouse_assemblies, &curtain_walls);
    let square_towers = derive_square_towers(program);
    let mut stairs = derive_stairs(program, &storeys, &towers);
    let battlements = derive_battlements(program);
    let wall_walks = derive_wall_walks(program, &battlements, &towers);
    let crowns = derive_crowns(program, &battlements, &towers);
    let defensive_junctions = derive_defensive_junctions(&wall_walks);
    let defensive_circuits = derive_defensive_circuits(program, &wall_walks);
    let tower_portals = derive_tower_portals(program, &towers, &wall_walks, &defensive_junctions);
    let mut resolved_geometry =
        resolve_crown_geometry(&crowns, &wall_walks, &stairs, &tower_portals);
    let gate_defenses = derive_gate_defenses(
        program,
        &gatehouse_assemblies,
        &towers,
        &curtain_walls,
        &wall_walks,
    );
    let bartizans = derive_bartizans(program);
    let projected_defenses = resolve_projected_defenses(
        program,
        &storeys,
        &battlements,
        &bartizans,
        &mut resolved_geometry,
    );
    let (mut wall_assemblies, mut opening_assemblies) = resolve_storey_wall_assemblies(
        program,
        &storeys,
        &projected_defenses,
        &mut resolved_geometry,
    );
    if program.archetype == BuildingArchetype::Cathedral {
        suppress_cathedral_legacy_storey_walls(
            &mut wall_assemblies,
            &mut opening_assemblies,
            &mut resolved_geometry,
        );
        resolve_cathedral_bell_stage(
            &square_towers,
            &mut wall_assemblies,
            &mut opening_assemblies,
            &mut resolved_geometry,
        );
    }
    let mut church = if program.archetype == BuildingArchetype::Cathedral {
        Some(resolve_church_assembly(
            program,
            &mut wall_assemblies,
            &mut opening_assemblies,
            &mut stairs,
            &mut resolved_geometry,
        ))
    } else {
        None
    };
    if matches!(
        program.archetype,
        BuildingArchetype::CastleGatehouse
            | BuildingArchetype::CourtyardCastle
            | BuildingArchetype::WalledKeep
            | BuildingArchetype::ArtilleryRondelCastle
    ) {
        resolve_round_tower_wall_assemblies(
            &towers,
            &crowns,
            &mut wall_assemblies,
            &mut resolved_geometry,
        );
        if matches!(
            program.archetype,
            BuildingArchetype::CastleGatehouse | BuildingArchetype::CourtyardCastle
        ) {
            replace_storey_wall_sources_inside_round_towers(
                &towers,
                &mut wall_assemblies,
                &mut opening_assemblies,
                &mut resolved_geometry,
            );
        }
        if program.archetype == BuildingArchetype::CastleGatehouse {
            resolve_gatehouse_tower_chord_bonds(
                &towers,
                &projected_defenses,
                &wall_assemblies,
                &mut resolved_geometry,
            );
        }
    }
    let artillery_castle = resolve_artillery_castle(
        program,
        &towers,
        &mut wall_assemblies,
        &mut opening_assemblies,
        &mut resolved_geometry,
    );

    let mut roof_assemblies = resolve_roof_assemblies(
        program,
        &roofs,
        &roof_dormers,
        &towers,
        &square_towers,
        &stairs,
        &wall_assemblies,
        &opening_assemblies,
        &mut resolved_geometry,
    );
    resolve_roof_child_front_openings(
        program,
        &roof_dormers,
        &mut roof_assemblies,
        &mut wall_assemblies,
        &mut opening_assemblies,
        &mut resolved_geometry,
    );
    let timber_frame = resolve_timber_frame_assembly(
        program,
        edits,
        &mut wall_assemblies,
        &opening_assemblies,
        &roofs,
        &roof_dormers,
        &stairs,
        &mut roof_assemblies,
        &mut resolved_geometry,
    );
    // Corner bonds must be resolved against the final timber-infill depth,
    // after the semantic frame has replaced the exterior structural layer.
    resolve_storey_wall_corner_bonds(&wall_assemblies, &mut resolved_geometry);
    if let Some(church) = &mut church {
        church.roof_assemblies = roof_assemblies.iter().map(|roof| roof.id).collect();
    }

    Ok(BuildingPlan {
        archetype: program.archetype,
        seed: program.seed,
        footprint: program.footprint,
        storey_height_metres: program.storey_height_metres,
        wall_style: program.wall_style,
        timber_frame_style: program.timber_frame_style,
        upper_storey_projection_metres: program.upper_storey_projection_metres,
        storeys,
        wall_assemblies,
        opening_assemblies,
        roofs,
        roof_dormers,
        roof_assemblies,
        towers,
        square_towers,
        stairs,
        battlements,
        crowns,
        projected_defenses,
        resolved_geometry,
        wall_walks,
        defensive_junctions,
        defensive_circuits,
        tower_portals,
        curtain_walls,
        gate_defenses,
        gatehouse_assemblies,
        bartizans,
        church,
        timber_frame,
        castle_phase: if program.archetype == BuildingArchetype::ArtilleryRondelCastle {
            Some(crate::CastleConstructionPhase::ArtilleryRetrofit1544)
        } else {
            matches!(
                program.archetype,
                BuildingArchetype::CastleGatehouse
                    | BuildingArchetype::CourtyardCastle
                    | BuildingArchetype::WalledKeep
            )
            .then_some(crate::CastleConstructionPhase::InheritedMedieval)
        },
        artillery_castle,
    })
}

fn apply_opening_edits(
    _storey_program: &crate::StoreyProgram,
    level: u16,
    walls: &[crate::WallSegment],
    openings: &mut Vec<Opening>,
    edits: &[BuildingEdit],
) -> Result<(), GenerationError> {
    for edit in edits {
        let selector = match edit {
            BuildingEdit::AddOpening { wall, .. } | BuildingEdit::RemoveOpening { wall } => *wall,
            BuildingEdit::SetWallStyle { .. } | BuildingEdit::SetTimberFrameStyle { .. } => {
                continue;
            }
        };
        if selector.storey_level != level {
            continue;
        }
        let wall_index = walls
            .iter()
            .position(|wall| wall.cell == selector.cell && wall.direction == selector.direction)
            .ok_or_else(|| {
                GenerationError::EditTargetNotFound(format!(
                    "storey {} cell ({}, {}) {:?} wall",
                    level, selector.cell.x, selector.cell.z, selector.direction
                ))
            })?;
        match *edit {
            BuildingEdit::AddOpening {
                opening_kind: kind,
                width_metres,
                sill_metres,
                height_metres,
                ..
            } => {
                if matches!(
                    kind,
                    OpeningKind::Window | OpeningKind::Gate | OpeningKind::ArrowSlit
                ) && !walls[wall_index].exterior()
                {
                    return Err(GenerationError::UnsupportedEdit(format!(
                        "{kind:?} openings require an exterior grid wall"
                    )));
                }
                if openings.iter().any(|opening| opening.wall == wall_index) {
                    return Err(GenerationError::EditConflict(format!(
                        "wall already owns an opening on storey {level}"
                    )));
                }
                let dimensions_are_valid = match kind {
                    OpeningKind::Window => {
                        (0.35..=1.20).contains(&width_metres)
                            && (0.30..=2.20).contains(&sill_metres)
                            && (0.45..=1.80).contains(&height_metres)
                    }
                    OpeningKind::Door => {
                        (0.70..=1.40).contains(&width_metres)
                            && sill_metres.abs() <= 0.01
                            && (1.80..=2.60).contains(&height_metres)
                    }
                    OpeningKind::Gate => {
                        (1.50..=3.80).contains(&width_metres)
                            && sill_metres.abs() <= 0.01
                            && (2.20..=3.40).contains(&height_metres)
                    }
                    OpeningKind::ArrowSlit => {
                        (0.15..=0.45).contains(&width_metres)
                            && (0.80..=1.80).contains(&sill_metres)
                            && (0.70..=1.50).contains(&height_metres)
                    }
                };
                if !dimensions_are_valid {
                    return Err(GenerationError::UnsupportedEdit(format!(
                        "{kind:?} dimensions are outside the editor project envelope"
                    )));
                }
                openings.push(Opening {
                    wall: wall_index,
                    kind,
                    width_metres,
                    sill_metres,
                    height_metres,
                });
                openings.sort_by_key(|opening| opening.wall);
            }
            BuildingEdit::RemoveOpening { .. } => {
                let before = openings.len();
                openings.retain(|opening| opening.wall != wall_index);
                if openings.len() == before {
                    return Err(GenerationError::EditTargetNotFound(format!(
                        "opening on storey {level} wall {wall_index}"
                    )));
                }
            }
            BuildingEdit::SetWallStyle { .. } | BuildingEdit::SetTimberFrameStyle { .. } => {}
        }
    }
    Ok(())
}

struct TimberFrameBuilder<'a> {
    geometry: &'a mut ResolvedGeometry,
    owner: GeometryOwnerId,
    material: crate::StructuralTimberMaterial,
    next_member: u64,
    next_node: u64,
    next_joint: u64,
    next_interface: u64,
    node_by_point: BTreeMap<(i32, i32, i32), StructuralNodeId>,
    joint_by_node: BTreeMap<StructuralNodeId, usize>,
    member_by_key: BTreeMap<(u8, (i32, i32, i32), (i32, i32, i32)), crate::TimberMemberId>,
    members: Vec<crate::TimberFrameMember>,
    joints: Vec<crate::TimberFrameJoint>,
}

impl<'a> TimberFrameBuilder<'a> {
    fn new(
        geometry: &'a mut ResolvedGeometry,
        owner: GeometryOwnerId,
        material: crate::StructuralTimberMaterial,
    ) -> Self {
        Self {
            geometry,
            owner,
            material,
            next_member: 1,
            next_node: 30_000_000,
            next_joint: 1,
            next_interface: 1,
            node_by_point: BTreeMap::new(),
            joint_by_node: BTreeMap::new(),
            member_by_key: BTreeMap::new(),
            members: Vec::new(),
            joints: Vec::new(),
        }
    }

    fn point_key(point: Vec3) -> (i32, i32, i32) {
        (
            (point.x * 1_000.0).round() as i32,
            (point.y * 1_000.0).round() as i32,
            (point.z * 1_000.0).round() as i32,
        )
    }

    fn node(&mut self, point: Vec3) -> StructuralNodeId {
        let key = Self::point_key(point);
        if let Some(id) = self.node_by_point.get(&key) {
            return *id;
        }
        let id = StructuralNodeId(self.next_node);
        self.next_node += 1;
        let grounded = point.y <= 0.011;
        self.geometry.structural_nodes.push(StructuralNode {
            id,
            owner: self.owner,
            kind: if grounded {
                StructuralNodeKind::TimberFrameFoundation
            } else {
                StructuralNodeKind::TimberFrameJoint
            },
            position: point,
            // Support edges are added only when a real member or measured
            // bearing interface is created. Spatial proximity is not a load
            // path: a nearby post must never support this node implicitly.
            supported_by: Vec::new(),
            grounded,
        });
        let joint_id = crate::TimberJointId(self.next_joint);
        self.next_joint += 1;
        self.joint_by_node.insert(id, self.joints.len());
        self.joints.push(crate::TimberFrameJoint {
            id: joint_id,
            node: id,
            kind: if grounded {
                crate::TimberJointKind::FoundationBearing
            } else {
                crate::TimberJointKind::MortiseTenon
            },
            member_ids: Vec::new(),
            contact_interfaces: Vec::new(),
            participants: Vec::new(),
            load_direction: Vec3::Y,
            contact_area_square_metres: 0.0144,
        });
        self.node_by_point.insert(key, id);
        id
    }

    fn solid_role(role: crate::TimberMemberRole) -> SolidRole {
        match role {
            crate::TimberMemberRole::Sill => SolidRole::FrameSill,
            crate::TimberMemberRole::PrimaryPost
            | crate::TimberMemberRole::CornerPost
            | crate::TimberMemberRole::IntermediatePost => SolidRole::FramePost,
            crate::TimberMemberRole::WallPlate => SolidRole::FramePlate,
            crate::TimberMemberRole::Rail => SolidRole::FrameRail,
            crate::TimberMemberRole::FloorJoist => SolidRole::FrameJoist,
            crate::TimberMemberRole::TransverseTie => SolidRole::FrameTie,
            crate::TimberMemberRole::Girder | crate::TimberMemberRole::Purlin => {
                SolidRole::FrameGirder
            }
            crate::TimberMemberRole::HeadBrace
            | crate::TimberMemberRole::FootBrace
            | crate::TimberMemberRole::StoreyBrace => SolidRole::FrameBrace,
            crate::TimberMemberRole::JettyBeam => SolidRole::FrameJettyBeam,
            crate::TimberMemberRole::Knagge => SolidRole::FrameKnagge,
            crate::TimberMemberRole::GableTie
            | crate::TimberMemberRole::GablePost
            | crate::TimberMemberRole::Rafter
            | crate::TimberMemberRole::Collar => SolidRole::FrameGableMember,
            crate::TimberMemberRole::DormerTrimmer => SolidRole::FrameDormerTrimmer,
            crate::TimberMemberRole::Ornament => SolidRole::FrameOrnament,
        }
    }

    fn member(
        &mut self,
        role: crate::TimberMemberRole,
        start: Vec3,
        end: Vec3,
        section: Vec2,
        phase: crate::TimberFramePhase,
    ) -> crate::TimberMemberId {
        let mut a = Self::point_key(start);
        let mut b = Self::point_key(end);
        if b < a {
            std::mem::swap(&mut a, &mut b);
        }
        // All vertical post labels share one physical member key.  A corner
        // referenced by two facade programs is still one timber, not nested
        // CornerPost/PrimaryPost solids occupying the same volume.
        let role_key = match role {
            crate::TimberMemberRole::PrimaryPost
            | crate::TimberMemberRole::CornerPost
            | crate::TimberMemberRole::IntermediatePost => {
                crate::TimberMemberRole::PrimaryPost as u8
            }
            _ => role as u8,
        };
        if let Some(id) = self.member_by_key.get(&(role_key, a, b)) {
            return *id;
        }
        let start_node = self.node(start);
        let end_node = self.node(end);
        if (start.y - end.y).abs() > 0.05 {
            let (upper, lower) = if start.y > end.y {
                (start_node, end_node)
            } else {
                (end_node, start_node)
            };
            if let Some(node) = self
                .geometry
                .structural_nodes
                .iter_mut()
                .find(|node| node.id == upper)
            {
                node.supported_by.push(lower);
                node.supported_by.sort_unstable();
                node.supported_by.dedup();
            }
        }
        let start_joint_index = self.joint_by_node[&start_node];
        let end_joint_index = self.joint_by_node[&end_node];
        let start_joint = self.joints[start_joint_index].id;
        let end_joint = self.joints[end_joint_index].id;
        let id = crate::TimberMemberId(self.next_member);
        self.next_member += 1;
        self.joints[start_joint_index].member_ids.push(id);
        if end_joint_index != start_joint_index {
            self.joints[end_joint_index].member_ids.push(id);
        }
        let delta = end - start;
        let length = delta.length();
        debug_assert!(length > 0.05);
        let horizontal = Vec2::new(delta.x, delta.z).length();
        let yaw = if horizontal > 0.001 {
            (-delta.z).atan2(delta.x)
        } else {
            0.0
        };
        let longfall = -horizontal.atan2(delta.y);
        let solid_id =
            ResolvedItemId((1_u64 << 60) | (u64::from(self.owner.0) << 32) | self.next_member);
        self.geometry.solids.push(ResolvedSolid {
            id: solid_id,
            owner: self.owner,
            centre: (start + end) * 0.5,
            size: Vec3::new(section.x, length, section.y),
            yaw_radians: yaw,
            crossfall_radians: 0.0,
            longfall_radians: longfall,
            role: Self::solid_role(role),
            shape: crate::ResolvedSolidShape::Cuboid,
            supported_by: vec![start_node, end_node],
        });
        let make_interface = |this: &mut Self, node, point: Vec3| {
            let interface = ResolvedItemId(
                (4_u64 << 60) | (u64::from(this.owner.0) << 32) | 0x100_000 | this.next_interface,
            );
            this.next_interface += 1;
            let half = Vec3::new(section.x, section.x.min(section.y), section.y) * 0.5;
            this.geometry.support_interfaces.push(SupportInterface {
                id: interface,
                owner: this.owner,
                node,
                bounds: ResolvedBounds {
                    min: point - half,
                    max: point + half,
                },
            });
            interface
        };
        let support_interfaces = [
            make_interface(self, start_node, start),
            make_interface(self, end_node, end),
        ];
        self.joints[start_joint_index]
            .contact_interfaces
            .push(support_interfaces[0]);
        if end_joint_index != start_joint_index {
            self.joints[end_joint_index]
                .contact_interfaces
                .push(support_interfaces[1]);
        }
        self.members.push(crate::TimberFrameMember {
            id,
            owner: self.owner,
            role,
            phase,
            material: self.material,
            start_node,
            end_node,
            start_joint,
            end_joint,
            start,
            end,
            section_metres: section,
            solid: solid_id,
            support_interfaces,
            structural: role != crate::TimberMemberRole::Ornament,
        });
        self.member_by_key.insert((role_key, a, b), id);
        id
    }

    /// Resolve a node which lands on the body of a post/brace to that exact
    /// member. Facade rails and opening headers commonly meet a continuous
    /// post between its end joints; the measured intersection is a housed or
    /// lap bearing, while mere nearby geometry is deliberately ignored.
    fn resolve_intermediate_member_bearings(&mut self) {
        let candidates = self
            .members
            .iter()
            .filter(|member| member.structural)
            .cloned()
            .collect::<Vec<_>>();
        let node_ids = self
            .geometry
            .structural_nodes
            .iter()
            .filter(|node| node.owner == self.owner && !node.grounded)
            .map(|node| node.id)
            .collect::<Vec<_>>();
        for node_id in node_ids {
            let Some(point) = self
                .geometry
                .structural_nodes
                .iter()
                .find(|node| node.id == node_id)
                .map(|node| node.position)
            else {
                continue;
            };
            let bearing = candidates
                .iter()
                .filter_map(|member| {
                    let delta = member.end - member.start;
                    let length_squared = delta.length_squared();
                    let t = ((point - member.start).dot(delta) / length_squared).clamp(0.0, 1.0);
                    if t <= 0.001 || t >= 0.999 {
                        return None;
                    }
                    let closest = member.start + delta * t;
                    let distance = closest.distance(point);
                    (distance <= member.section_metres.min_element() * 0.55 + 0.004)
                        .then_some((member, distance))
                })
                .min_by(|left, right| left.1.total_cmp(&right.1));
            let Some((member, _)) = bearing else { continue };
            let lower = if member.start.y <= member.end.y {
                member.start_node
            } else {
                member.end_node
            };
            if let Some(node) = self
                .geometry
                .structural_nodes
                .iter_mut()
                .find(|node| node.id == node_id)
            {
                node.supported_by.push(lower);
            }
            let interface = ResolvedItemId(
                (4_u64 << 60) | (u64::from(self.owner.0) << 32) | 0x180_000 | self.next_interface,
            );
            self.next_interface += 1;
            let half = Vec3::new(
                member.section_metres.x,
                member.section_metres.min_element(),
                member.section_metres.y,
            ) * 0.45;
            self.geometry.support_interfaces.push(SupportInterface {
                id: interface,
                owner: self.owner,
                node: node_id,
                bounds: ResolvedBounds {
                    min: point - half,
                    max: point + half,
                },
            });
        }
    }

    /// Orient the physical timber-member/contact graph into an acyclic load
    /// tree rooted at foundations. Each resulting support edge is either a
    /// member endpoint pair or an exact point-on-member housed bearing created
    /// above; no distance-based inference participates.
    fn rebuild_physical_support_tree(&mut self) {
        let node_ids = self
            .geometry
            .structural_nodes
            .iter()
            .filter(|node| node.owner == self.owner)
            .map(|node| node.id)
            .collect::<BTreeSet<_>>();
        let mut adjacency = BTreeMap::<StructuralNodeId, BTreeSet<StructuralNodeId>>::new();
        for member in self.members.iter().filter(|member| member.structural) {
            adjacency
                .entry(member.start_node)
                .or_default()
                .insert(member.end_node);
            adjacency
                .entry(member.end_node)
                .or_default()
                .insert(member.start_node);
        }
        // Preserve exact intermediate point-on-member contacts and exact
        // contacts with externally grounded wall/roof authorities. The latter
        // are roots of this frame graph, not inferred nearby supports.
        let body_contacts = self
            .geometry
            .structural_nodes
            .iter()
            .filter(|node| node.owner == self.owner && !node.grounded)
            .flat_map(|node| {
                node.supported_by
                    .iter()
                    .map(move |parent| (node.id, *parent))
            })
            .collect::<Vec<_>>();
        let mut external_roots = Vec::new();
        for (node, parent) in body_contacts {
            if node_ids.contains(&parent) {
                adjacency.entry(node).or_default().insert(parent);
                adjacency.entry(parent).or_default().insert(node);
            } else {
                external_roots.push((node, parent));
            }
        }
        for node in self
            .geometry
            .structural_nodes
            .iter_mut()
            .filter(|node| node.owner == self.owner)
        {
            node.supported_by.clear();
        }
        for (node_id, parent) in &external_roots {
            if let Some(node) = self
                .geometry
                .structural_nodes
                .iter_mut()
                .find(|node| node.id == *node_id)
            {
                node.supported_by.push(*parent);
            }
        }
        let mut roots = self
            .geometry
            .structural_nodes
            .iter()
            .filter(|node| node.owner == self.owner && node.grounded)
            .map(|node| node.id)
            .collect::<Vec<_>>();
        roots.extend(external_roots.iter().map(|(node, _)| *node));
        roots.sort_unstable();
        roots.dedup();
        let mut visited = roots.iter().copied().collect::<BTreeSet<_>>();
        let mut queue = VecDeque::from(roots);
        while let Some(parent) = queue.pop_front() {
            for child in adjacency.get(&parent).into_iter().flatten().copied() {
                if !node_ids.contains(&child) || !visited.insert(child) {
                    continue;
                }
                if let Some(node) = self
                    .geometry
                    .structural_nodes
                    .iter_mut()
                    .find(|node| node.id == child)
                {
                    node.supported_by.push(parent);
                }
                queue.push_back(child);
            }
        }
    }

    /// Assign the compact Stage 6 joint vocabulary from the physical members
    /// which actually meet at each node. Decorative or proximity-only labels
    /// never participate in load transfer.
    fn classify_physical_joints(&mut self) {
        for joint in &mut self.joints {
            joint.contact_interfaces.sort_unstable();
            joint.contact_interfaces.dedup();
            let roles = joint
                .member_ids
                .iter()
                .filter_map(|id| self.members.iter().find(|member| member.id == *id))
                .map(|member| member.role)
                .collect::<Vec<_>>();
            let grounded = self
                .geometry
                .structural_nodes
                .iter()
                .find(|node| node.id == joint.node)
                .is_some_and(|node| node.grounded);
            let has = |role| roles.contains(&role);
            joint.kind = if grounded {
                crate::TimberJointKind::FoundationBearing
            } else if (has(crate::TimberMemberRole::JettyBeam)
                && (has(crate::TimberMemberRole::Knagge)
                    || has(crate::TimberMemberRole::Girder)
                    || has(crate::TimberMemberRole::Sill)))
                || (has(crate::TimberMemberRole::Knagge)
                    && (has(crate::TimberMemberRole::PrimaryPost)
                        || has(crate::TimberMemberRole::CornerPost)))
            {
                crate::TimberJointKind::JettyBearing
            } else if (has(crate::TimberMemberRole::Rafter)
                && (has(crate::TimberMemberRole::WallPlate)
                    || has(crate::TimberMemberRole::Collar)
                    || has(crate::TimberMemberRole::GablePost)))
                || (has(crate::TimberMemberRole::DormerTrimmer)
                    && (has(crate::TimberMemberRole::Rafter)
                        || has(crate::TimberMemberRole::Purlin)))
                || (has(crate::TimberMemberRole::Purlin)
                    && (has(crate::TimberMemberRole::PrimaryPost)
                        || has(crate::TimberMemberRole::GablePost)))
            {
                crate::TimberJointKind::RoofSeat
            } else if (has(crate::TimberMemberRole::FloorJoist)
                && has(crate::TimberMemberRole::Girder))
                || (has(crate::TimberMemberRole::TransverseTie)
                    && (has(crate::TimberMemberRole::PrimaryPost)
                        || has(crate::TimberMemberRole::Purlin)))
            {
                crate::TimberJointKind::HousedBeam
            } else if roles.iter().any(|role| {
                matches!(
                    role,
                    crate::TimberMemberRole::HeadBrace
                        | crate::TimberMemberRole::FootBrace
                        | crate::TimberMemberRole::StoreyBrace
                )
            }) && roles.len() >= 2
            {
                crate::TimberJointKind::Lap
            } else if roles
                .iter()
                .filter(|role| {
                    matches!(
                        role,
                        crate::TimberMemberRole::Sill | crate::TimberMemberRole::WallPlate
                    )
                })
                .count()
                >= 2
            {
                crate::TimberJointKind::Scarf
            } else {
                crate::TimberJointKind::MortiseTenon
            };
            joint.participants = joint
                .member_ids
                .iter()
                .filter_map(|member_id| {
                    let member = self.members.iter().find(|member| member.id == *member_id)?;
                    let axis = if member.start_node == joint.node {
                        member.end - member.start
                    } else if member.end_node == joint.node {
                        member.start - member.end
                    } else {
                        return None;
                    }
                    .normalize_or_zero();
                    Some(crate::TimberJointParticipant {
                        member: *member_id,
                        axis_from_joint: axis,
                        reaction_direction: -axis,
                    })
                })
                .collect();
            let role_axis = |role| {
                joint.participants.iter().find_map(|participant| {
                    self.members
                        .iter()
                        .find(|member| member.id == participant.member && member.role == role)
                        .map(|_| participant.axis_from_joint)
                })
            };
            let downward = |axis: Vec3| if axis.y <= 0.0 { axis } else { -axis };
            let gravity_biased = |axis: Vec3, lateral_weight: f32| {
                let lateral = Vec3::new(axis.x, 0.0, axis.z).normalize_or_zero();
                (lateral * lateral_weight - Vec3::Y).normalize_or_zero()
            };
            joint.load_direction = match joint.kind {
                crate::TimberJointKind::JettyBearing => {
                    role_axis(crate::TimberMemberRole::JettyBeam)
                        .map(|axis| gravity_biased(axis, 0.65))
                }
                crate::TimberJointKind::Lap => [
                    crate::TimberMemberRole::HeadBrace,
                    crate::TimberMemberRole::FootBrace,
                    crate::TimberMemberRole::StoreyBrace,
                ]
                .into_iter()
                .find_map(role_axis)
                .map(downward),
                crate::TimberJointKind::RoofSeat => role_axis(crate::TimberMemberRole::Rafter)
                    .or_else(|| role_axis(crate::TimberMemberRole::Purlin))
                    .map(downward),
                crate::TimberJointKind::HousedBeam => {
                    role_axis(crate::TimberMemberRole::FloorJoist)
                        .or_else(|| role_axis(crate::TimberMemberRole::TransverseTie))
                        .map(|axis| gravity_biased(axis, 0.25))
                }
                crate::TimberJointKind::Scarf => role_axis(crate::TimberMemberRole::Sill)
                    .or_else(|| role_axis(crate::TimberMemberRole::WallPlate))
                    .map(|axis| gravity_biased(axis, 0.20)),
                _ => joint
                    .participants
                    .iter()
                    .max_by(|left, right| {
                        left.axis_from_joint
                            .y
                            .abs()
                            .total_cmp(&right.axis_from_joint.y.abs())
                    })
                    .map(|participant| {
                        if participant.axis_from_joint.y.abs() >= 0.35 {
                            downward(participant.axis_from_joint)
                        } else {
                            gravity_biased(participant.axis_from_joint, 0.15)
                        }
                    }),
            }
            .unwrap_or(-Vec3::Y)
            .normalize_or_zero();
        }
    }
}

fn timber_program_kind(archetype: BuildingArchetype) -> Option<crate::TimberFrameProgramKind> {
    Some(match archetype {
        BuildingArchetype::TownHouse => crate::TimberFrameProgramKind::NarrowUrbanTownHouse,
        BuildingArchetype::HallHouse => crate::TimberFrameProgramKind::NorthernTwoPostHallHouse,
        BuildingArchetype::FachwerkCottage => crate::TimberFrameProgramKind::DirectRoofCottage,
        BuildingArchetype::FachwerkMerchantHouse => {
            crate::TimberFrameProgramKind::JettiedMerchantHouse
        }
        BuildingArchetype::RenaissanceTownHall => {
            crate::TimberFrameProgramKind::CivicMasonryTimberHall
        }
        _ => return None,
    })
}

fn closed_polygon(points: impl IntoIterator<Item = Vec2>) -> Polygon<f32> {
    let mut coords = points
        .into_iter()
        .map(|point| Coord {
            x: point.x,
            y: point.y,
        })
        .collect::<Vec<_>>();
    if coords.first() != coords.last() {
        coords.push(coords[0]);
    }
    Polygon::new(LineString::new(coords), Vec::new())
}

fn timber_member_wall_polygon(
    member: &crate::TimberFrameMember,
    wall: &crate::WallAssembly,
) -> Polygon<f32> {
    let project = |point: Vec3| {
        Vec2::new(
            (Vec2::new(point.x, point.z) - wall.frame.origin).dot(wall.frame.tangent),
            point.y - wall.base_elevation_metres,
        )
    };
    let start = project(member.start);
    let end = project(member.end);
    let axis = (end - start).normalize_or_zero();
    let normal = Vec2::new(-axis.y, axis.x);
    let half = member.section_metres.max_element() * 0.5;
    closed_polygon([
        start - axis * half - normal * half,
        end + axis * half - normal * half,
        end + axis * half + normal * half,
        start - axis * half + normal * half,
    ])
}

fn triangulate_panel_polygon(polygon: &Polygon<f32>) -> Vec<[Vec2; 3]> {
    let mut vertices = polygon
        .exterior()
        .0
        .iter()
        .take(polygon.exterior().0.len().saturating_sub(1))
        .map(|coord| Vec2::new(coord.x, coord.y))
        .collect::<Vec<_>>();
    let mut holes = Vec::new();
    for interior in polygon.interiors() {
        holes.push(vertices.len() as u32);
        vertices.extend(
            interior
                .0
                .iter()
                .take(interior.0.len().saturating_sub(1))
                .map(|coord| Vec2::new(coord.x, coord.y)),
        );
    }
    let mut indices = Vec::new();
    earcut::Earcut::<f32>::new().earcut(
        vertices.iter().map(|point| [point.x, point.y]),
        &holes,
        &mut indices,
    );
    indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(|triangle| {
            let points = [
                vertices[triangle[0] as usize],
                vertices[triangle[1] as usize],
                vertices[triangle[2] as usize],
            ];
            (((points[1] - points[0]).perp_dot(points[2] - points[0])).abs() > 0.000_01)
                .then_some(points)
        })
        .collect()
}

fn resolve_timber_frame_assembly(
    program: &BuildingProgram,
    edits: &[BuildingEdit],
    walls: &mut [crate::WallAssembly],
    openings: &[crate::OpeningAssembly],
    roofs: &[RoofPiece],
    dormers: &[RoofDormer],
    stairs: &[Stair],
    roof_assemblies: &mut [RoofAssembly],
    geometry: &mut ResolvedGeometry,
) -> Option<crate::TimberFrameAssembly> {
    let program_kind = timber_program_kind(program.archetype)?;
    let owner = GeometryOwnerId(82_000);
    let frame_material = if matches!(
        program_kind,
        crate::TimberFrameProgramKind::NorthernTwoPostHallHouse
            | crate::TimberFrameProgramKind::DirectRoofCottage
    ) {
        crate::StructuralTimberMaterial::Oak
    } else {
        crate::StructuralTimberMaterial::Fir
    };
    let mut builder = TimberFrameBuilder::new(geometry, owner, frame_material);
    let mut facades = Vec::new();
    let mut bays = Vec::new();
    let mut next_facade = 1_u64;
    let mut next_line = 1_u64;
    let mut next_storey = 1_u64;
    let mut next_bay = 1_u64;
    let section = Vec2::splat(if program.archetype == BuildingArchetype::FachwerkCottage {
        0.13
    } else {
        0.15
    });
    let directions = [
        Direction::South,
        Direction::East,
        Direction::North,
        Direction::West,
    ];
    for direction in directions {
        let outward = direction_vector(direction);
        let tangent = if outward.y.abs() > 0.5 {
            Vec2::X
        } else {
            Vec2::Y
        };
        let mut line_storeys = Vec::new();
        let mut line_origin = Vec2::ZERO;
        let mut line_length = 0.0_f32;
        for level in 0..program.storeys.len() as u16 {
            let mut facade_walls = walls
                .iter()
                .filter(|wall| {
                    wall.storey_level == level
                        && wall.frame.outside_room.is_none()
                        && wall.frame.outward.dot(outward) > 0.99
                        && wall.material == crate::WallMaterialClass::TimberInfill
                        && matches!(wall.source, crate::WallSourceId::StoreyWall { .. })
                })
                .collect::<Vec<_>>();
            facade_walls.sort_by(|left, right| {
                left.frame
                    .origin
                    .dot(tangent)
                    .total_cmp(&right.frame.origin.dot(tangent))
            });
            if facade_walls.is_empty() {
                continue;
            }
            line_origin = facade_walls
                .iter()
                .map(|wall| wall.frame.origin)
                .sum::<Vec2>()
                / facade_walls.len() as f32;
            line_length = facade_walls.len() as f32 * CELL_SIZE_METRES;
            let base = f32::from(level) * program.storey_height_metres;
            let top = base + program.storey_height_metres;
            let mut storey_member_ids = Vec::new();
            let mut bay_ids = Vec::new();
            for (wall_index, wall) in facade_walls.iter().enumerate() {
                let plane = wall.frame.origin
                    + wall.frame.outward * (wall.thickness_metres * 0.5 - section.y * 0.5);
                let left_plan = plane - tangent * wall.length_metres * 0.5;
                let right_plan = plane + tangent * wall.length_metres * 0.5;
                let left_bottom = Vec3::new(left_plan.x, base, left_plan.y);
                let right_bottom = Vec3::new(right_plan.x, base, right_plan.y);
                let left_top = Vec3::new(left_plan.x, top, left_plan.y);
                let right_top = Vec3::new(right_plan.x, top, right_plan.y);
                let mut member_ids = vec![
                    builder.member(
                        crate::TimberMemberRole::Sill,
                        left_bottom,
                        right_bottom,
                        section,
                        crate::TimberFramePhase::PrimaryConstruction,
                    ),
                    builder.member(
                        crate::TimberMemberRole::WallPlate,
                        left_top,
                        right_top,
                        section,
                        crate::TimberFramePhase::PrimaryConstruction,
                    ),
                    builder.member(
                        if wall_index == 0 {
                            crate::TimberMemberRole::CornerPost
                        } else {
                            crate::TimberMemberRole::PrimaryPost
                        },
                        left_bottom,
                        left_top,
                        section,
                        crate::TimberFramePhase::PrimaryConstruction,
                    ),
                    builder.member(
                        if wall_index + 1 == facade_walls.len() {
                            crate::TimberMemberRole::CornerPost
                        } else {
                            crate::TimberMemberRole::PrimaryPost
                        },
                        right_bottom,
                        right_top,
                        section,
                        crate::TimberFramePhase::PrimaryConstruction,
                    ),
                ];
                let opening = wall
                    .opening_ids
                    .first()
                    .and_then(|id| openings.iter().find(|opening| opening.id == *id));
                if let Some(opening) = opening {
                    let void_bounds = builder
                        .geometry
                        .voids
                        .iter()
                        .find(|void| void.id == opening.void_id)
                        .map(|void| void.bounds);
                    let half = void_bounds.map_or_else(
                        || opening.profile.interior_width_metres() * 0.5,
                        |bounds| {
                            let size = bounds.max - bounds.min;
                            (size.x * tangent.x.abs() + size.z * tangent.y.abs()) * 0.5
                        },
                    );
                    let (sill, head) = void_bounds.map_or_else(
                        || {
                            (
                                opening.sill_elevation_metres,
                                opening.sill_elevation_metres
                                    + opening.profile.clear_height_metres(),
                            )
                        },
                        |bounds| (bounds.min.y, bounds.max.y),
                    );
                    let left_jamb_plan = plane - tangent * half;
                    let right_jamb_plan = plane + tangent * half;
                    let left_jamb_bottom = Vec3::new(left_jamb_plan.x, base, left_jamb_plan.y);
                    let right_jamb_bottom = Vec3::new(right_jamb_plan.x, base, right_jamb_plan.y);
                    let left_jamb_top = Vec3::new(left_jamb_plan.x, top, left_jamb_plan.y);
                    let right_jamb_top = Vec3::new(right_jamb_plan.x, top, right_jamb_plan.y);
                    member_ids.extend([
                        builder.member(
                            crate::TimberMemberRole::IntermediatePost,
                            left_jamb_bottom,
                            left_jamb_top,
                            section * 0.9,
                            crate::TimberFramePhase::PrimaryConstruction,
                        ),
                        builder.member(
                            crate::TimberMemberRole::IntermediatePost,
                            right_jamb_bottom,
                            right_jamb_top,
                            section * 0.9,
                            crate::TimberFramePhase::PrimaryConstruction,
                        ),
                        builder.member(
                            crate::TimberMemberRole::Rail,
                            Vec3::new(left_jamb_plan.x, sill, left_jamb_plan.y),
                            Vec3::new(right_jamb_plan.x, sill, right_jamb_plan.y),
                            section * 0.88,
                            crate::TimberFramePhase::PrimaryConstruction,
                        ),
                        builder.member(
                            crate::TimberMemberRole::Rail,
                            Vec3::new(
                                left_jamb_plan.x,
                                head + section.x * 0.5 + 0.01,
                                left_jamb_plan.y,
                            ),
                            Vec3::new(
                                right_jamb_plan.x,
                                head + section.x * 0.5 + 0.01,
                                right_jamb_plan.y,
                            ),
                            section,
                            crate::TimberFramePhase::PrimaryConstruction,
                        ),
                    ]);
                    // Each side panel owns a closed triangular racking frame:
                    // the paired braces share one explicit jamb node and the
                    // corner post closes the third side.  The former foot/head
                    // braces stopped at unrelated sill/head nodes, so they
                    // looked plausible but could not transmit racking load.
                    let brace_joint_y = (sill + head) * 0.5;
                    let left_brace_joint =
                        Vec3::new(left_jamb_plan.x, brace_joint_y, left_jamb_plan.y);
                    let right_brace_joint =
                        Vec3::new(right_jamb_plan.x, brace_joint_y, right_jamb_plan.y);
                    member_ids.extend([
                        builder.member(
                            crate::TimberMemberRole::FootBrace,
                            left_bottom,
                            left_brace_joint,
                            section * 0.74,
                            crate::TimberFramePhase::PrimaryConstruction,
                        ),
                        builder.member(
                            crate::TimberMemberRole::HeadBrace,
                            left_brace_joint,
                            left_top,
                            section * 0.70,
                            crate::TimberFramePhase::PrimaryConstruction,
                        ),
                        builder.member(
                            crate::TimberMemberRole::FootBrace,
                            right_bottom,
                            right_brace_joint,
                            section * 0.74,
                            crate::TimberFramePhase::PrimaryConstruction,
                        ),
                        builder.member(
                            crate::TimberMemberRole::HeadBrace,
                            right_brace_joint,
                            right_top,
                            section * 0.70,
                            crate::TimberFramePhase::PrimaryConstruction,
                        ),
                    ]);
                } else {
                    let centre_plan = (left_plan + right_plan) * 0.5;
                    let centre_bottom = Vec3::new(centre_plan.x, base, centre_plan.y);
                    let centre_top = Vec3::new(centre_plan.x, top, centre_plan.y);
                    member_ids.push(builder.member(
                        crate::TimberMemberRole::IntermediatePost,
                        centre_bottom,
                        centre_top,
                        section * 0.78,
                        crate::TimberFramePhase::PrimaryConstruction,
                    ));
                    let waist = base + program.storey_height_metres * 0.56;
                    member_ids.push(builder.member(
                        crate::TimberMemberRole::Rail,
                        Vec3::new(left_plan.x, waist, left_plan.y),
                        Vec3::new(right_plan.x, waist, right_plan.y),
                        section * 0.78,
                        crate::TimberFramePhase::PrimaryConstruction,
                    ));
                    let editor_style = edits.iter().rev().find_map(|edit| match edit {
                        BuildingEdit::SetTimberFrameStyle { style } => Some(*style),
                        _ => None,
                    });
                    let rising = editor_style.map_or_else(
                        || (wall_index + usize::from(level)).is_multiple_of(2),
                        |style| match style {
                            crate::TimberFrameStyle::LateMedieval => {
                                (wall_index + usize::from(level)).is_multiple_of(2)
                            }
                            crate::TimberFrameStyle::NorthernCloseStudded => {
                                wall_index.is_multiple_of(2)
                            }
                            crate::TimberFrameStyle::EarlyModernOrnate => {
                                (wall_index / 2 + usize::from(level)).is_multiple_of(2)
                            }
                        },
                    );
                    let (brace_start, brace_end) = if rising {
                        (left_bottom, right_top)
                    } else {
                        (right_bottom, left_top)
                    };
                    member_ids.push(builder.member(
                        crate::TimberMemberRole::StoreyBrace,
                        brace_start,
                        brace_end,
                        section * 0.76,
                        crate::TimberFramePhase::PrimaryConstruction,
                    ));
                }
                member_ids.sort_unstable();
                member_ids.dedup();
                let bay_id = crate::TimberFrameBayId(next_bay);
                next_bay += 1;
                bay_ids.push(bay_id);
                storey_member_ids.extend(member_ids.iter().copied());
                bays.push(crate::TimberFrameBay {
                    id: bay_id,
                    wall: Some(wall.id),
                    opening: opening.map(|opening| opening.id),
                    member_ids,
                    infill_solids: wall
                        .host_solids
                        .iter()
                        .copied()
                        .filter(|id| {
                            builder.geometry.solids.iter().any(|solid| {
                                solid.id == *id
                                    && matches!(
                                        solid.role,
                                        SolidRole::WallHost
                                            | SolidRole::OpeningJamb
                                            | SolidRole::OpeningSill
                                            | SolidRole::OpeningHead
                                            | SolidRole::OpeningSpandrel
                                    )
                            })
                        })
                        .collect(),
                });
            }
            storey_member_ids.sort_unstable();
            storey_member_ids.dedup();
            let jetty = if level == 1 && program.upper_storey_projection_metres > 0.01 {
                let projection = program.upper_storey_projection_metres;
                let backspan = 0.95_f32;
                let mut jetty_beams = Vec::new();
                let mut knaggen = Vec::new();
                let mut corner_supports = Vec::new();
                for (index, wall) in facade_walls.iter().enumerate() {
                    let plane = wall.frame.origin
                        + wall.frame.outward * (wall.thickness_metres * 0.5 - section.y * 0.5);
                    for sign in [-1.0_f32, 1.0] {
                        let boundary = plane + tangent * sign * wall.length_metres * 0.5;
                        let outer = Vec3::new(boundary.x, base, boundary.y);
                        let inner_plan = boundary - outward * (projection + backspan);
                        let inner = Vec3::new(inner_plan.x, base, inner_plan.y);
                        let beam = builder.member(
                            crate::TimberMemberRole::JettyBeam,
                            inner,
                            outer,
                            section,
                            crate::TimberFramePhase::UpperStoreyAddition,
                        );
                        jetty_beams.push(beam);
                        let lower_plan = boundary - outward * projection;
                        let lower = Vec3::new(
                            lower_plan.x,
                            base - program.storey_height_metres * 0.28,
                            lower_plan.y,
                        );
                        let knagge = builder.member(
                            crate::TimberMemberRole::Knagge,
                            lower,
                            outer,
                            section * 0.9,
                            crate::TimberFramePhase::UpperStoreyAddition,
                        );
                        knaggen.push(knagge);
                        if index == 0 || index + 1 == facade_walls.len() {
                            corner_supports.push(knagge);
                        }
                    }
                }
                jetty_beams.sort_unstable();
                jetty_beams.dedup();
                let mut inner_bearings = jetty_beams
                    .iter()
                    .filter_map(|id| builder.members.iter().find(|member| member.id == *id))
                    .map(|member| member.start)
                    .collect::<Vec<_>>();
                inner_bearings.sort_by(|left, right| {
                    Vec2::new(left.x, left.z)
                        .dot(tangent)
                        .total_cmp(&Vec2::new(right.x, right.z).dot(tangent))
                });
                if let (Some(first), Some(last)) = (
                    inner_bearings.first().copied(),
                    inner_bearings.last().copied(),
                ) && first.distance(last) > 0.10
                {
                    let inner_girder = builder.member(
                        crate::TimberMemberRole::Girder,
                        first,
                        last,
                        section * 1.12,
                        crate::TimberFramePhase::UpperStoreyAddition,
                    );
                    storey_member_ids.push(inner_girder);
                }
                knaggen.sort_unstable();
                knaggen.dedup();
                corner_supports.sort_unstable();
                corner_supports.dedup();
                storey_member_ids.extend(jetty_beams.iter().copied());
                storey_member_ids.extend(knaggen.iter().copied());
                let outer_plane = facade_walls
                    .iter()
                    .map(|wall| {
                        wall.frame.origin
                            + wall.frame.outward * (wall.thickness_metres * 0.5 - section.y * 0.5)
                    })
                    .sum::<Vec2>()
                    / facade_walls.len() as f32;
                // Only the projecting strip is a separate jetty plate. The
                // backspan remains part of the main storey floor assembled
                // below, avoiding duplicate overlapping floor authority.
                let floor_depth = projection;
                let floor_centre_plan = outer_plane - outward * floor_depth * 0.5;
                let floor_solid = ResolvedItemId(
                    (1_u64 << 60) | (u64::from(owner.0) << 32) | 0x0f00_0000 | next_storey,
                );
                let floor_support_nodes = jetty_beams
                    .iter()
                    .filter_map(|id| builder.members.iter().find(|member| member.id == *id))
                    .flat_map(|member| [member.start_node, member.end_node])
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                builder.geometry.solids.push(ResolvedSolid {
                    id: floor_solid,
                    owner,
                    centre: Vec3::new(floor_centre_plan.x, base - 0.08, floor_centre_plan.y),
                    size: Vec3::new(line_length, 0.16, floor_depth),
                    yaw_radians: (-tangent.y).atan2(tangent.x),
                    crossfall_radians: 0.0,
                    longfall_radians: 0.0,
                    role: SolidRole::FrameFloor,
                    shape: crate::ResolvedSolidShape::Cuboid,
                    supported_by: floor_support_nodes,
                });
                let mut floor_bearing_interfaces = Vec::new();
                for member in jetty_beams
                    .iter()
                    .filter_map(|id| builder.members.iter().find(|member| member.id == *id))
                {
                    let inward = (member.start - member.end).normalize_or_zero();
                    let contact = member.end + inward * (projection * 0.5) - Vec3::Y * 0.04;
                    let interface = ResolvedItemId(
                        (4_u64 << 60)
                            | (u64::from(owner.0) << 32)
                            | 0x300_000
                            | builder.next_interface,
                    );
                    builder.next_interface += 1;
                    builder.geometry.support_interfaces.push(SupportInterface {
                        id: interface,
                        owner,
                        node: member.end_node,
                        bounds: ResolvedBounds {
                            min: contact - Vec3::new(0.07, 0.025, 0.07),
                            max: contact + Vec3::new(0.07, 0.025, 0.07),
                        },
                    });
                    floor_bearing_interfaces.push(interface);
                }
                let half_length = line_length * 0.5;
                let left_outer = outer_plane - tangent * half_length;
                let right_outer = outer_plane + tangent * half_length;
                let structural_depth = projection + backspan;
                let left_inner = left_outer - outward * structural_depth;
                let right_inner = right_outer - outward * structural_depth;
                Some(crate::TimberJettyAssembly {
                    projection_metres: projection,
                    backspan_metres: backspan,
                    jetty_beams,
                    knaggen,
                    corner_supports,
                    floor_solid,
                    floor_bearing_interfaces,
                    support_polygon: vec![left_inner, right_inner, right_outer, left_outer],
                })
            } else {
                None
            };
            line_storeys.push(crate::TimberStoreyFrame {
                id: crate::TimberStoreyFrameId(next_storey),
                level,
                kind: match (program_kind, level) {
                    (crate::TimberFrameProgramKind::DirectRoofCottage, _) => {
                        crate::TimberStoreyKind::GroundFrame
                    }
                    (crate::TimberFrameProgramKind::CivicMasonryTimberHall, 0) => {
                        crate::TimberStoreyKind::MasonryPlinth
                    }
                    (crate::TimberFrameProgramKind::CivicMasonryTimberHall, _) => {
                        crate::TimberStoreyKind::CivicTimberHall
                    }
                    (_, 0) => crate::TimberStoreyKind::GroundFrame,
                    _ => crate::TimberStoreyKind::UpperFrame,
                },
                base_elevation_metres: base,
                top_elevation_metres: top,
                bay_ids,
                member_ids: storey_member_ids,
                jetty,
            });
            next_storey += 1;
        }
        if !line_storeys.is_empty() {
            facades.push(crate::TimberFrameFacade {
                id: crate::TimberFacadeId(next_facade),
                outward: direction,
                lines: vec![crate::TimberFrameLine {
                    id: crate::TimberFrameLineId(next_line),
                    origin: line_origin,
                    tangent,
                    outward,
                    length_metres: line_length,
                    internal: false,
                    storeys: line_storeys,
                }],
            });
            next_facade += 1;
            next_line += 1;
        }
    }

    let dimensions = Vec2::new(
        f32::from(program.footprint.dimensions().0) * CELL_SIZE_METRES,
        f32::from(program.footprint.dimensions().1) * CELL_SIZE_METRES,
    );
    let mut internal_lines = Vec::new();
    if program_kind == crate::TimberFrameProgramKind::NorthernTwoPostHallHouse {
        let ridge_x = roofs
            .first()
            .is_none_or(|roof| roof.ridge_axis == RidgeAxis::X);
        let tangent = if ridge_x { Vec2::X } else { Vec2::Y };
        let cross = Vec2::new(-tangent.y, tangent.x);
        let centre = dimensions * 0.5;
        // The two longitudinal post rows terminate inside the gable enclosure;
        // they bear the roof without piercing an opening or the weather skin.
        // 0.60 m end clearances are a coarse animation/buildability gate.
        let length = (if ridge_x { dimensions.x } else { dimensions.y } - 1.20).max(3.0);
        let row_offset = if ridge_x { dimensions.y } else { dimensions.x } * 0.20;
        for side in [-1.0_f32, 1.0] {
            let row_centre = centre + cross * row_offset * side;
            let count = (length / 3.0).ceil() as usize;
            let mut member_ids = Vec::new();
            for index in 0..=count {
                let along = -length * 0.5 + length * index as f32 / count as f32;
                let plan = row_centre + tangent * along;
                member_ids.push(builder.member(
                    crate::TimberMemberRole::PrimaryPost,
                    Vec3::new(plan.x, 0.0, plan.y),
                    Vec3::new(plan.x, program.storey_height_metres, plan.y),
                    section * 1.15,
                    crate::TimberFramePhase::PrimaryConstruction,
                ));
                if index < count {
                    let next_along = -length * 0.5 + length * (index + 1) as f32 / count as f32;
                    let next = row_centre + tangent * next_along;
                    let brace_start = Vec3::new(plan.x, 0.0, plan.y);
                    let brace_end = Vec3::new(next.x, program.storey_height_metres, next.y);
                    let crosses_opening = openings.iter().any(|opening| {
                        builder
                            .geometry
                            .voids
                            .iter()
                            .find(|void| void.id == opening.void_id)
                            .is_some_and(|void| {
                                (0..=32).any(|sample| {
                                    let point = brace_start.lerp(brace_end, sample as f32 / 32.0);
                                    point.x >= void.bounds.min.x - 0.08
                                        && point.x <= void.bounds.max.x + 0.08
                                        && point.y >= void.bounds.min.y - 0.08
                                        && point.y <= void.bounds.max.y + 0.08
                                        && point.z >= void.bounds.min.z - 0.08
                                        && point.z <= void.bounds.max.z + 0.08
                                })
                            })
                    });
                    if !crosses_opening {
                        member_ids.push(builder.member(
                            crate::TimberMemberRole::FootBrace,
                            brace_start,
                            brace_end,
                            section * 0.82,
                            crate::TimberFramePhase::PrimaryConstruction,
                        ));
                    }
                }
            }
            for index in 0..count {
                let a_along = -length * 0.5 + length * index as f32 / count as f32;
                let b_along = -length * 0.5 + length * (index + 1) as f32 / count as f32;
                let a = row_centre + tangent * a_along;
                let b = row_centre + tangent * b_along;
                member_ids.push(builder.member(
                    crate::TimberMemberRole::Purlin,
                    Vec3::new(a.x, program.storey_height_metres, a.y),
                    Vec3::new(b.x, program.storey_height_metres, b.y),
                    section * 1.2,
                    crate::TimberFramePhase::RoofConstruction,
                ));
            }
            internal_lines.push(crate::TimberFrameLine {
                id: crate::TimberFrameLineId(next_line),
                origin: row_centre,
                tangent,
                outward: cross * side,
                length_metres: length,
                internal: true,
                storeys: vec![crate::TimberStoreyFrame {
                    id: crate::TimberStoreyFrameId(next_storey),
                    level: 0,
                    kind: crate::TimberStoreyKind::GroundFrame,
                    base_elevation_metres: 0.0,
                    top_elevation_metres: program.storey_height_metres,
                    bay_ids: Vec::new(),
                    member_ids,
                    jetty: None,
                }],
            });
            next_line += 1;
            next_storey += 1;
        }
        let tie_count = (length / 3.0).ceil() as usize;
        for index in 0..=tie_count {
            let along = -length * 0.5 + length * index as f32 / tie_count as f32;
            let plan = centre + tangent * along;
            let a = plan - cross * row_offset;
            let b = plan + cross * row_offset;
            let (brace_start, brace_end) = if index.is_multiple_of(2) {
                (
                    Vec3::new(a.x, 0.0, a.y),
                    Vec3::new(b.x, program.storey_height_metres, b.y),
                )
            } else {
                (
                    Vec3::new(b.x, 0.0, b.y),
                    Vec3::new(a.x, program.storey_height_metres, a.y),
                )
            };
            let crosses_opening = openings.iter().any(|opening| {
                builder
                    .geometry
                    .voids
                    .iter()
                    .find(|void| void.id == opening.void_id)
                    .is_some_and(|void| {
                        (0..=32).any(|sample| {
                            let point = brace_start.lerp(brace_end, sample as f32 / 32.0);
                            point.x >= void.bounds.min.x - 0.08
                                && point.x <= void.bounds.max.x + 0.08
                                && point.y >= void.bounds.min.y - 0.08
                                && point.y <= void.bounds.max.y + 0.08
                                && point.z >= void.bounds.min.z - 0.08
                                && point.z <= void.bounds.max.z + 0.08
                        })
                    })
            });
            if crosses_opening {
                continue;
            }
            let tie = builder.member(
                crate::TimberMemberRole::TransverseTie,
                Vec3::new(a.x, program.storey_height_metres, a.y),
                Vec3::new(b.x, program.storey_height_metres, b.y),
                section * 1.1,
                crate::TimberFramePhase::RoofConstruction,
            );
            let brace = builder.member(
                crate::TimberMemberRole::StoreyBrace,
                brace_start,
                brace_end,
                section * 0.82,
                crate::TimberFramePhase::PrimaryConstruction,
            );
            let mut transverse_members = vec![tie, brace];
            transverse_members.extend(builder.members.iter().filter_map(|member| {
                (member.role == crate::TimberMemberRole::PrimaryPost
                    && ((member.start.distance(Vec3::new(a.x, 0.0, a.y)) <= 0.003
                        && member
                            .end
                            .distance(Vec3::new(a.x, program.storey_height_metres, a.y))
                            <= 0.003)
                        || (member.start.distance(Vec3::new(b.x, 0.0, b.y)) <= 0.003
                            && member.end.distance(Vec3::new(
                                b.x,
                                program.storey_height_metres,
                                b.y,
                            )) <= 0.003)))
                    .then_some(member.id)
            }));
            transverse_members.sort_unstable();
            transverse_members.dedup();
            internal_lines.push(crate::TimberFrameLine {
                id: crate::TimberFrameLineId(next_line),
                origin: plan,
                tangent: cross,
                outward: tangent,
                length_metres: row_offset * 2.0,
                internal: true,
                storeys: vec![crate::TimberStoreyFrame {
                    id: crate::TimberStoreyFrameId(next_storey),
                    level: 0,
                    kind: crate::TimberStoreyKind::GroundFrame,
                    base_elevation_metres: 0.0,
                    top_elevation_metres: program.storey_height_metres,
                    bay_ids: Vec::new(),
                    member_ids: transverse_members,
                    jetty: None,
                }],
            });
            next_line += 1;
            next_storey += 1;
        }
    }

    let top = program.storeys.len() as f32 * program.storey_height_metres;
    // Facade corner posts are storey-height segments with shared end joints.
    // Do not overlay them with a second ground-to-roof post: that former
    // shortcut created nested positive-volume timbers and two competing load
    // authorities at every corner.
    if let Some(roof) = roofs.first() {
        let half_width = if roof.ridge_axis == RidgeAxis::X {
            roof.size.y * 0.5
        } else {
            roof.size.x * 0.5
        };
        let rise = half_width * roof.pitch_degrees.to_radians().tan();
        let ridge_tangent = if roof.ridge_axis == RidgeAxis::X {
            Vec2::X
        } else {
            Vec2::Y
        };
        let gable_tangent = Vec2::new(-ridge_tangent.y, ridge_tangent.x);
        let half_length = if roof.ridge_axis == RidgeAxis::X {
            roof.size.x * 0.5
        } else {
            roof.size.y * 0.5
        };
        let frame_count = ((half_length * 2.0) / 1.80).ceil().max(1.0) as usize;
        let mut roof_frames = Vec::new();
        for frame_index in 0..=frame_count {
            let along = -half_length + half_length * 2.0 * frame_index as f32 / frame_count as f32;
            let gable_centre = roof.centre + ridge_tangent * along;
            if frame_index != 0
                && frame_index != frame_count
                && dormers.iter().any(|dormer| {
                    (dormer.centre - gable_centre).dot(ridge_tangent).abs()
                        <= dormer.width_metres * 0.5 + 0.40
                })
            {
                // The child roof owns its cut and four-sided trimmer frame;
                // a regular parent truss may not continue through that cut.
                continue;
            }
            let left = gable_centre - gable_tangent * half_width;
            let right = gable_centre + gable_tangent * half_width;
            // A half-hip does not have the full ridge elevation at its end
            // frames.  The former full-height A-frame recipe was structurally
            // grounded but projected through the two upper hip faces.  Match
            // the Stage 4 half-hip construction: the retained lower gable
            // reaches 55% of the rise at the end, then the frame apex climbs
            // along the short hip to the main ridge.
            let station_rise = if roof.kind == RoofKind::HalfHip {
                let hip_run = (half_width * 0.45).max(0.001);
                let distance_from_end = (half_length - along.abs()).max(0.0);
                rise * (0.55 + 0.45 * (distance_from_end / hip_run).clamp(0.0, 1.0))
            } else {
                rise
            };
            let apex = Vec3::new(gable_centre.x, top + station_rise, gable_centre.y);
            let left_base = Vec3::new(left.x, top, left.y);
            let right_base = Vec3::new(right.x, top, right.y);
            builder.member(
                crate::TimberMemberRole::GableTie,
                left_base,
                right_base,
                section,
                crate::TimberFramePhase::RoofConstruction,
            );
            builder.member(
                crate::TimberMemberRole::GablePost,
                Vec3::new(gable_centre.x, top, gable_centre.y),
                apex,
                section,
                crate::TimberFramePhase::RoofConstruction,
            );
            let collar_y = top + station_rise * 0.58;
            let collar_half = half_width * (1.0 - 0.58);
            let collar_left = gable_centre - gable_tangent * collar_half;
            let collar_right = gable_centre + gable_tangent * collar_half;
            let collar_left = Vec3::new(collar_left.x, collar_y, collar_left.y);
            let collar_right = Vec3::new(collar_right.x, collar_y, collar_right.y);
            for (base, collar) in [(left_base, collar_left), (right_base, collar_right)] {
                builder.member(
                    crate::TimberMemberRole::Rafter,
                    base,
                    collar,
                    section * 0.9,
                    crate::TimberFramePhase::RoofConstruction,
                );
                builder.member(
                    crate::TimberMemberRole::Rafter,
                    collar,
                    apex,
                    section * 0.9,
                    crate::TimberFramePhase::RoofConstruction,
                );
            }
            builder.member(
                crate::TimberMemberRole::Collar,
                collar_left,
                collar_right,
                section * 0.82,
                crate::TimberFramePhase::RoofConstruction,
            );
            roof_frames.push((collar_left, apex, collar_right));
        }
        for pair in roof_frames.windows(2) {
            for (left, right) in [
                (pair[0].0, pair[1].0),
                (pair[0].1, pair[1].1),
                (pair[0].2, pair[1].2),
            ] {
                builder.member(
                    crate::TimberMemberRole::Purlin,
                    left,
                    right,
                    section * 1.05,
                    crate::TimberFramePhase::RoofConstruction,
                );
            }
        }
    }

    let mut dormer_trimmer_members = Vec::new();
    for (dormer_index, dormer) in dormers.iter().enumerate() {
        let outward = direction_vector(dormer.facing);
        let tangent = Vec2::new(outward.y, -outward.x);
        let mut local_trimmers = Vec::new();
        // A facade-derived transverse gable starts at the facade and cuts
        // inward.  Treating its source centre like an ordinary dormer centre
        // put half the curb outside the wall/eave and directly through the
        // accepted roof drainage fall line.
        // Every attached child starts at its visible front wall and extends
        // inward through the parent slope.  The old ordinary-dormer curb was
        // centred on that front wall (-0.45..+0.45), leaving half of its
        // trimmers visibly cantilevered out over the parent covering.  Share
        // the exact front/rear datum used by the child enclosure instead.
        let roof_id = RoofAssemblyId(1_000 + dormer_index as u64);
        let exact_cut = roof_assemblies
            .iter()
            .flat_map(|roof| &roof.children)
            .find(|child| child.child == roof_id)
            .and_then(|child| {
                builder
                    .geometry
                    .voids
                    .iter()
                    .find(|void| void.id == child.parent_cut)
            });
        let (rear_depth, front_depth) = exact_cut.map_or((-0.84_f32, 0.0_f32), |cut| {
            let projected = [
                Vec2::new(cut.bounds.min.x, cut.bounds.min.z),
                Vec2::new(cut.bounds.min.x, cut.bounds.max.z),
                Vec2::new(cut.bounds.max.x, cut.bounds.min.z),
                Vec2::new(cut.bounds.max.x, cut.bounds.max.z),
            ]
            .map(|point| (point - dormer.centre).dot(outward) / dormer.depth_metres);
            (
                projected.iter().copied().fold(f32::INFINITY, f32::min),
                projected.iter().copied().fold(f32::NEG_INFINITY, f32::max),
            )
        });
        let trimmer_height = |point: Vec2, depth: f32| {
            roof_assemblies
                .iter()
                .find(|roof| roof.parent.is_none())
                .and_then(|roof| roof_underside_height_at(roof, point))
                .unwrap_or(
                    dormer.base_height_metres
                        - if dormer.kind == DormerKind::TransverseGable && depth == front_depth {
                            0.18
                        } else {
                            0.0
                        },
                )
        };
        for side in [-1.0_f32, 1.0] {
            let offset = tangent * side * dormer.width_metres * 0.5;
            let start = dormer.centre + offset + outward * dormer.depth_metres * rear_depth;
            let end = dormer.centre + offset + outward * dormer.depth_metres * front_depth;
            let trimmer = builder.member(
                crate::TimberMemberRole::DormerTrimmer,
                Vec3::new(start.x, trimmer_height(start, rear_depth), start.y),
                Vec3::new(end.x, trimmer_height(end, front_depth), end.y),
                section * 0.9,
                crate::TimberFramePhase::RoofConstruction,
            );
            dormer_trimmer_members.push(trimmer);
            local_trimmers.push(trimmer);
        }
        // The two longitudinal trimmers are tied into front and rear headers,
        // forming an authoritative four-sided curb around the Stage 4 parent
        // cut. This gives the child cheeks/front a closed load-transfer frame
        // instead of two independently floating roof bars.
        for depth in [rear_depth, front_depth] {
            let centre = dormer.centre + outward * dormer.depth_metres * depth;
            let start = centre - tangent * dormer.width_metres * 0.5;
            let end = centre + tangent * dormer.width_metres * 0.5;
            let trimmer = builder.member(
                crate::TimberMemberRole::DormerTrimmer,
                Vec3::new(start.x, trimmer_height(start, depth), start.y),
                Vec3::new(end.x, trimmer_height(end, depth), end.y),
                section * 0.9,
                crate::TimberFramePhase::RoofConstruction,
            );
            dormer_trimmer_members.push(trimmer);
            local_trimmers.push(trimmer);
        }
        let child_wall = walls.iter().find(|wall| {
            matches!(wall.source, crate::WallSourceId::RoofChildFront { roof } if roof == roof_id)
                && wall.material == crate::WallMaterialClass::TimberInfill
        });
        if let Some(wall) = child_wall {
            let opening = wall
                .opening_ids
                .first()
                .and_then(|id| openings.iter().find(|opening| opening.id == *id));
            let plane = wall.frame.origin
                + wall.frame.outward * (wall.thickness_metres * 0.5 - section.y * 0.5);
            let half = wall.length_metres * 0.5;
            let left = plane - tangent * half;
            let right = plane + tangent * half;
            let base = wall.base_elevation_metres;
            let top = base + wall.height_metres;
            let mut member_ids = local_trimmers;
            member_ids.extend([
                builder.member(
                    crate::TimberMemberRole::Sill,
                    Vec3::new(left.x, base, left.y),
                    Vec3::new(right.x, base, right.y),
                    section * 0.9,
                    crate::TimberFramePhase::RoofConstruction,
                ),
                builder.member(
                    crate::TimberMemberRole::WallPlate,
                    Vec3::new(left.x, top, left.y),
                    Vec3::new(right.x, top, right.y),
                    section * 0.9,
                    crate::TimberFramePhase::RoofConstruction,
                ),
            ]);
            // The opening jamb posts below carry the compact dormer front.
            // Do not add a second pair of full-height corner posts: aligned
            // with the facade below, those read as free columns piercing the
            // parent roof.  Continue the frame above the eave as an explicit
            // triangular gable instead.
            if let Some(ridge_y) = (dormer.kind != DormerKind::Shed)
                .then(|| {
                    roof_assemblies
                        .iter()
                        .find(|roof| roof.id == roof_id)
                        .into_iter()
                        .flat_map(|roof| roof.faces.iter())
                        .flat_map(|face| face.polygon.iter())
                        .map(|point| point.y)
                        .max_by(f32::total_cmp)
                })
                .flatten()
            {
                let apex = Vec3::new(plane.x, ridge_y, plane.y);
                let eave_centre = Vec3::new(plane.x, top, plane.y);
                member_ids.extend([
                    builder.member(
                        crate::TimberMemberRole::GablePost,
                        eave_centre,
                        apex,
                        section * 0.72,
                        crate::TimberFramePhase::RoofConstruction,
                    ),
                    builder.member(
                        crate::TimberMemberRole::Rafter,
                        Vec3::new(left.x, top, left.y),
                        apex,
                        section * 0.78,
                        crate::TimberFramePhase::RoofConstruction,
                    ),
                    builder.member(
                        crate::TimberMemberRole::Rafter,
                        Vec3::new(right.x, top, right.y),
                        apex,
                        section * 0.78,
                        crate::TimberFramePhase::RoofConstruction,
                    ),
                ]);
            }
            if let Some(opening) = opening
                && let Some(void_bounds) = builder
                    .geometry
                    .voids
                    .iter()
                    .find(|void| void.id == opening.void_id)
                    .map(|void| void.bounds)
            {
                let opening_half = opening.profile.interior_width_metres() * 0.5;
                let left_jamb = plane - tangent * opening_half;
                let right_jamb = plane + tangent * opening_half;
                member_ids.extend([
                    builder.member(
                        crate::TimberMemberRole::IntermediatePost,
                        Vec3::new(left_jamb.x, base, left_jamb.y),
                        Vec3::new(left_jamb.x, top, left_jamb.y),
                        section * 0.78,
                        crate::TimberFramePhase::RoofConstruction,
                    ),
                    builder.member(
                        crate::TimberMemberRole::IntermediatePost,
                        Vec3::new(right_jamb.x, base, right_jamb.y),
                        Vec3::new(right_jamb.x, top, right_jamb.y),
                        section * 0.78,
                        crate::TimberFramePhase::RoofConstruction,
                    ),
                    builder.member(
                        crate::TimberMemberRole::Rail,
                        Vec3::new(left_jamb.x, void_bounds.min.y, left_jamb.y),
                        Vec3::new(right_jamb.x, void_bounds.min.y, right_jamb.y),
                        section * 0.75,
                        crate::TimberFramePhase::RoofConstruction,
                    ),
                    builder.member(
                        crate::TimberMemberRole::Rail,
                        Vec3::new(left_jamb.x, void_bounds.max.y, left_jamb.y),
                        Vec3::new(right_jamb.x, void_bounds.max.y, right_jamb.y),
                        section * 0.85,
                        crate::TimberFramePhase::RoofConstruction,
                    ),
                ]);
            }
            member_ids.sort_unstable();
            member_ids.dedup();
            let bay_id = crate::TimberFrameBayId(next_bay);
            next_bay += 1;
            bays.push(crate::TimberFrameBay {
                id: bay_id,
                wall: Some(wall.id),
                opening: opening.map(|opening| opening.id),
                member_ids,
                infill_solids: wall.host_solids.clone(),
            });
        }
    }

    // Replace monolithic Stage 3 WallHost leaves with bay-local infill
    // panels. Opening jamb/head/sill/spandrel solids retain their independent
    // bearing authority; these residual panels cover only the wall field
    // around the opening and sit behind the structural timber layer.
    let mut removed_panel_ids = std::collections::HashSet::new();
    for wall in walls
        .iter_mut()
        .filter(|wall| wall.material == crate::WallMaterialClass::TimberInfill)
    {
        let old_panels = wall
            .host_solids
            .iter()
            .copied()
            .filter(|id| {
                builder
                    .geometry
                    .solids
                    .iter()
                    .any(|solid| solid.id == *id && solid.role == SolidRole::WallHost)
            })
            .collect::<Vec<_>>();
        removed_panel_ids.extend(old_panels.iter().copied());
        wall.host_solids.retain(|id| !old_panels.contains(id));

        let half_length = wall.length_metres * 0.5;
        let field = closed_polygon([
            Vec2::new(-half_length, 0.0),
            Vec2::new(half_length, 0.0),
            Vec2::new(half_length, wall.height_metres),
            Vec2::new(-half_length, wall.height_metres),
        ]);
        let mut residual = MultiPolygon(vec![field]);
        for opening in openings
            .iter()
            .filter(|opening| opening.host_wall == wall.id)
        {
            let half_opening =
                (opening.profile.interior_width_metres() * 0.5).min(half_length - 0.02);
            let centre = (opening.frame.origin - wall.frame.origin).dot(wall.frame.tangent);
            let sill = (opening.sill_elevation_metres - wall.base_elevation_metres)
                .clamp(0.0, wall.height_metres);
            let head =
                (sill + opening.profile.clear_height_metres()).clamp(sill, wall.height_metres);
            let opening_polygon = closed_polygon([
                Vec2::new(centre - half_opening, sill),
                Vec2::new(centre + half_opening, sill),
                Vec2::new(centre + half_opening, head),
                Vec2::new(centre - half_opening, head),
            ]);
            residual = residual.difference(&opening_polygon);
        }
        let wall_member_ids = bays
            .iter()
            .filter(|bay| bay.wall == Some(wall.id))
            .flat_map(|bay| bay.member_ids.iter().copied())
            .collect::<std::collections::HashSet<_>>();
        for member in builder
            .members
            .iter()
            .filter(|member| wall_member_ids.contains(&member.id))
        {
            residual = residual.difference(&timber_member_wall_polygon(member, wall));
        }

        let panel_depth = (wall.thickness_metres - section.y).max(0.04);
        // Stage 3 opening-bearing solids retain the structural wall depth, but
        // their exposed face is recessed from the Fachwerk plane. Their exact
        // overlap with the opening's jamb/header members is a typed composite
        // opening-frame relation audited below; unrelated timber receives no
        // such permission.
        let opening_recess = 0.012_f32.min(wall.thickness_metres - 0.04);
        let inward = -wall.frame.outward;
        for solid in builder.geometry.solids.iter_mut().filter(|solid| {
            wall.host_solids.contains(&solid.id)
                && matches!(
                    solid.role,
                    SolidRole::OpeningJamb
                        | SolidRole::OpeningSill
                        | SolidRole::OpeningHead
                        | SolidRole::OpeningSpandrel
                )
        }) {
            solid.centre += Vec3::new(inward.x, 0.0, inward.y) * opening_recess * 0.5;
            if wall.frame.outward.x.abs() > 0.5 {
                solid.size.x = (solid.size.x - opening_recess).max(0.04);
            } else {
                solid.size.z = (solid.size.z - opening_recess).max(0.04);
            }
        }
        let mut panel_ids = Vec::new();
        let triangles = residual
            .0
            .iter()
            .flat_map(triangulate_panel_polygon)
            .collect::<Vec<_>>();
        for (index, triangle) in triangles.into_iter().enumerate() {
            let id = ResolvedItemId(
                (1_u64 << 60) | (u64::from(wall.owner.0) << 32) | 0x0f00_0000 | index as u64,
            );
            let contact = ResolvedItemId(
                (4_u64 << 60) | (u64::from(wall.owner.0) << 32) | 0x0f00_0000 | index as u64,
            );
            let mid_plane = wall.frame.origin - wall.frame.outward * (section.y * 0.5);
            let vertices = triangle.map(|point| {
                let plan = mid_plane + wall.frame.tangent * point.x;
                Vec3::new(plan.x, wall.base_elevation_metres + point.y, plan.y)
            });
            let depth_offset =
                Vec3::new(wall.frame.outward.x, 0.0, wall.frame.outward.y) * panel_depth * 0.5;
            let min = vertices
                .iter()
                .flat_map(|vertex| [*vertex - depth_offset, *vertex + depth_offset])
                .fold(Vec3::splat(f32::INFINITY), Vec3::min);
            let max = vertices
                .iter()
                .flat_map(|vertex| [*vertex - depth_offset, *vertex + depth_offset])
                .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
            let centre = (min + max) * 0.5;
            let size = max - min;
            builder.geometry.solids.push(ResolvedSolid {
                id,
                owner: wall.owner,
                centre,
                size,
                yaw_radians: 0.0,
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::WallHost,
                shape: crate::ResolvedSolidShape::TimberPanelPrism {
                    vertices,
                    outward: wall.frame.outward,
                    depth_metres: panel_depth,
                },
                supported_by: vec![wall.support_node],
            });
            builder.geometry.support_interfaces.push(SupportInterface {
                id: contact,
                owner: wall.owner,
                node: wall.support_node,
                bounds: ResolvedBounds {
                    min: Vec3::new(
                        centre.x - size.x * 0.5,
                        centre.y - size.y * 0.5 - 0.004,
                        centre.z - size.z * 0.5,
                    ),
                    max: Vec3::new(
                        centre.x + size.x * 0.5,
                        centre.y - size.y * 0.5 + 0.008,
                        centre.z + size.z * 0.5,
                    ),
                },
            });
            wall.host_solids.push(id);
            panel_ids.push(id);
        }
        for bay in bays.iter_mut().filter(|bay| bay.wall == Some(wall.id)) {
            bay.infill_solids = panel_ids.clone();
        }
    }
    builder
        .geometry
        .solids
        .retain(|solid| !removed_panel_ids.contains(&solid.id));

    let (preferred_stair_origin, preferred_stair_axis, stair_width, stair_run) = stairs
        .iter()
        .find_map(|stair| match *stair {
            Stair::Straight {
                start,
                direction,
                width_metres,
                ..
            } => Some((
                start,
                direction_vector(direction),
                // The opening includes the two stringers outside the one
                // metre occupant prism. `stair_width` is the structural cut;
                // the route/void below remains the clear one-metre core.
                width_metres.max(1.0) + 0.36,
                3.20_f32.min(dimensions.max_element() - 1.0),
            )),
            Stair::Spiral { .. } => None,
        })
        .unwrap_or((
            Vec2::new(dimensions.x * 0.5, dimensions.y * 0.5 - 2.1),
            Vec2::Y,
            1.36,
            (dimensions.y - 1.0).clamp(2.8, 4.2),
        ));
    let collect_wall_bounds = |ground_route_only: bool| {
        walls
            .iter()
            .flat_map(|wall| &wall.host_solids)
            .filter_map(|id| {
                builder
                    .geometry
                    .solids
                    .iter()
                    .find(|solid| {
                        solid.id == *id
                            && (!ground_route_only
                                || (solid.centre.y - solid.size.y * 0.5 < 1.90
                                    && solid.centre.y + solid.size.y * 0.5 > 0.02))
                    })
                    .map(|solid| {
                        let cosine = solid.yaw_radians.cos().abs();
                        let sine = solid.yaw_radians.sin().abs();
                        let half = Vec3::new(
                            (solid.size.x * cosine + solid.size.z * sine) * 0.5,
                            solid.size.y * 0.5,
                            (solid.size.x * sine + solid.size.z * cosine) * 0.5,
                        );
                        let min = solid.centre - half;
                        let max = solid.centre + half;
                        (Vec2::new(min.x, min.z), Vec2::new(max.x, max.z))
                    })
            })
            .collect::<Vec<_>>()
    };
    let stair_wall_bounds = collect_wall_bounds(false);
    let ground_route_wall_bounds = collect_wall_bounds(true);
    let mut stair_candidates = vec![(preferred_stair_origin, preferred_stair_axis)];
    for z in 1..((dimensions.y / CELL_SIZE_METRES) as i32) {
        for x in 1..((dimensions.x / CELL_SIZE_METRES) as i32) {
            let origin = Vec2::new(
                (x as f32 + 0.5) * CELL_SIZE_METRES,
                (z as f32 + 0.5) * CELL_SIZE_METRES,
            );
            for axis in [Vec2::Y, Vec2::X, -Vec2::Y, -Vec2::X] {
                stair_candidates.push((origin, axis));
            }
        }
    }
    stair_candidates.sort_by(|left, right| {
        left.0
            .distance(preferred_stair_origin)
            .total_cmp(&right.0.distance(preferred_stair_origin))
    });
    let (stair_origin, stair_axis) = stair_candidates
        .into_iter()
        .find(|(origin, axis)| {
            let end = *origin + *axis * stair_run;
            let lateral = Vec2::new(-axis.y, axis.x);
            let side = lateral * stair_width * 0.5;
            let min = (*origin - side)
                .min(*origin + side)
                .min(end - side)
                .min(end + side);
            let max = (*origin - side)
                .max(*origin + side)
                .max(end - side)
                .max(end + side);
            min.cmpge(Vec2::splat(0.20)).all()
                && max.cmple(dimensions - Vec2::splat(0.20)).all()
                && stair_wall_bounds.iter().all(|(wall_min, wall_max)| {
                    max.x <= wall_min.x + 0.01
                        || min.x >= wall_max.x - 0.01
                        || max.y <= wall_min.y + 0.01
                        || min.y >= wall_max.y - 0.01
                })
        })
        .unwrap_or((preferred_stair_origin, preferred_stair_axis));
    let stair_lateral = Vec2::new(-stair_axis.y, stair_axis.x);
    let stair_end = stair_origin + stair_axis * stair_run;
    let side = stair_lateral * stair_width * 0.5;
    let stair_min = (stair_origin - side)
        .min(stair_origin + side)
        .min(stair_end - side)
        .min(stair_end + side);
    let stair_max = (stair_origin - side)
        .max(stair_origin + side)
        .max(stair_end - side)
        .max(stair_end + side);
    let stair_min = stair_min.max(Vec2::splat(0.20));
    let stair_max = stair_max.min(dimensions - Vec2::splat(0.20));
    let stair_floor_cut = |level: u16| {
        let (flight_origin, flight_axis) = if level % 2 == 1 {
            (stair_origin, stair_axis)
        } else {
            (stair_end, -stair_axis)
        };
        let flight_lateral = Vec2::new(-flight_axis.y, flight_axis.x);
        let flight_end = flight_origin + flight_axis * stair_run;
        let clear_side = flight_lateral * 0.50;
        let cut_inner = flight_end - flight_axis * 0.30;
        let cut_outer = flight_end - flight_axis * 0.11;
        let clear_min = (cut_inner - clear_side)
            .min(cut_inner + clear_side)
            .min(cut_outer - clear_side)
            .min(cut_outer + clear_side);
        let clear_max = (cut_inner - clear_side)
            .max(cut_inner + clear_side)
            .max(cut_outer - clear_side)
            .max(cut_outer + clear_side);
        (clear_min, clear_max)
    };

    let mut floors = Vec::new();
    for level in 0..program.storeys.len() as u16 {
        let base = f32::from(level) * program.storey_height_metres;
        let mut girder_members = Vec::new();
        let mut joist_members = Vec::new();
        let mut bearing_interfaces = Vec::new();
        let mut floor_joist_interfaces = Vec::new();
        let mut joist_girder_interfaces = Vec::new();
        let joist_count = (dimensions.x / 1.35).ceil().max(2.0) as usize;
        let mut x_stations = (0..=joist_count)
            .map(|index| 0.20 + (dimensions.x - 0.40) * index as f32 / joist_count as f32)
            .collect::<Vec<_>>();
        let cut_bounds = (level > 0).then(|| stair_floor_cut(level));
        if let Some((cut_min, cut_max)) = cut_bounds {
            x_stations.extend([cut_min.x, cut_max.x]);
            x_stations.sort_by(f32::total_cmp);
            x_stations.dedup_by(|left, right| (*left - *right).abs() < 0.08);
        }
        let mut upper_girder_z = dimensions.y * 0.67;
        if (upper_girder_z - stair_end.y).abs() < 0.40 {
            upper_girder_z = (stair_end.y + 0.40).min(dimensions.y - 0.40);
        }
        let support_z_stations = [
            0.20_f32,
            dimensions.y * 0.33,
            upper_girder_z,
            dimensions.y - 0.20,
        ];
        let mut girder_z_stations = vec![support_z_stations[1], support_z_stations[2]];
        if level > 0 {
            girder_z_stations.extend([stair_min.y, stair_max.y]);
            girder_z_stations.sort_by(f32::total_cmp);
            girder_z_stations.dedup_by(|left, right| (*left - *right).abs() < 0.08);
        }
        let mut joist_z_stations = support_z_stations.to_vec();
        if level > 0 {
            joist_z_stations.extend([stair_min.y, stair_max.y]);
            joist_z_stations.sort_by(f32::total_cmp);
            joist_z_stations.dedup_by(|left, right| (*left - *right).abs() < 0.08);
        }
        let joist_section = section * 0.90;
        let girder_section = Vec2::new(section.x * 1.35, section.y * 1.20);
        let bearing_y = base - 0.16 - joist_section.y * 0.5;
        let mut floor_supports = Vec::new();
        if level > 0 {
            // Split both orthogonal member families at every crossing. Their
            // shared structural node is therefore a physical housed bearing,
            // not an interface floating at a member midpoint.
            for z in &girder_z_stations {
                for pair in x_stations.windows(2) {
                    if cut_bounds.is_some_and(|(cut_min, cut_max)| {
                        *z > cut_min.y - girder_section.x * 0.5
                            && *z < cut_max.y + girder_section.x * 0.5
                            && (pair[0] + pair[1]) * 0.5 > cut_min.x
                            && (pair[0] + pair[1]) * 0.5 < cut_max.x
                    }) {
                        continue;
                    }
                    girder_members.push(builder.member(
                        crate::TimberMemberRole::Girder,
                        Vec3::new(pair[0], bearing_y, *z),
                        Vec3::new(pair[1], bearing_y, *z),
                        girder_section,
                        crate::TimberFramePhase::PrimaryConstruction,
                    ));
                }
                for x in [x_stations[0], *x_stations.last().expect("floor x station")] {
                    let lower_y = if level == 1 {
                        0.0
                    } else {
                        f32::from(level - 1) * program.storey_height_metres
                            - 0.16
                            - joist_section.y * 0.5
                    };
                    builder.member(
                        crate::TimberMemberRole::PrimaryPost,
                        Vec3::new(x, lower_y, *z),
                        Vec3::new(x, bearing_y, *z),
                        section,
                        crate::TimberFramePhase::PrimaryConstruction,
                    );
                }
            }
            for x in &x_stations {
                for pair in joist_z_stations.windows(2) {
                    let midpoint = Vec2::new(*x, (pair[0] + pair[1]) * 0.5);
                    if midpoint.x > stair_min.x + 0.001
                        && midpoint.x < stair_max.x - 0.001
                        && midpoint.y > stair_min.y + 0.001
                        && midpoint.y < stair_max.y - 0.001
                    {
                        continue;
                    }
                    joist_members.push(builder.member(
                        crate::TimberMemberRole::FloorJoist,
                        Vec3::new(*x, bearing_y, pair[0]),
                        Vec3::new(*x, bearing_y, pair[1]),
                        joist_section,
                        crate::TimberFramePhase::PrimaryConstruction,
                    ));
                }
                for z in &girder_z_stations {
                    let z = *z;
                    let at = Vec3::new(*x, bearing_y, z);
                    let point_on = |member: &crate::TimberFrameMember| {
                        let axis = member.end - member.start;
                        let t = (at - member.start).dot(axis) / axis.length_squared().max(0.0001);
                        (-0.001..=1.001).contains(&t)
                            && member
                                .start
                                .lerp(member.end, t.clamp(0.0, 1.0))
                                .distance(at)
                                <= 0.003
                    };
                    if !joist_members.iter().any(|id| {
                        builder
                            .members
                            .iter()
                            .find(|member| member.id == *id)
                            .is_some_and(point_on)
                    }) || !girder_members.iter().any(|id| {
                        builder
                            .members
                            .iter()
                            .find(|member| member.id == *id)
                            .is_some_and(point_on)
                    }) {
                        continue;
                    }
                    let node = builder.node(Vec3::new(*x, bearing_y, z));
                    floor_supports.push(node);
                    let housed = ResolvedItemId(
                        (4_u64 << 60)
                            | (u64::from(owner.0) << 32)
                            | 0x390_000
                            | builder.next_interface,
                    );
                    builder.next_interface += 1;
                    builder.geometry.support_interfaces.push(SupportInterface {
                        id: housed,
                        owner,
                        node,
                        bounds: ResolvedBounds {
                            min: Vec3::new(
                                *x - joist_section.x * 0.5,
                                bearing_y - joist_section.y * 0.5,
                                z - girder_section.x * 0.5,
                            ),
                            max: Vec3::new(
                                *x + joist_section.x * 0.5,
                                bearing_y + joist_section.y * 0.5,
                                z + girder_section.x * 0.5,
                            ),
                        },
                    });
                    joist_girder_interfaces.push(housed);
                    if *x >= stair_min.x - 0.001
                        && *x <= stair_max.x + 0.001
                        && z >= stair_min.y - 0.001
                        && z <= stair_max.y + 0.001
                    {
                        bearing_interfaces.push(housed);
                        continue;
                    }
                    let floor_contact = ResolvedItemId(
                        (4_u64 << 60)
                            | (u64::from(owner.0) << 32)
                            | 0x3a0_000
                            | builder.next_interface,
                    );
                    builder.next_interface += 1;
                    let contact_y = base - 0.16;
                    builder.geometry.support_interfaces.push(SupportInterface {
                        id: floor_contact,
                        owner,
                        node,
                        bounds: ResolvedBounds {
                            min: Vec3::new(*x - 0.055, contact_y - 0.004, z - 0.16),
                            max: Vec3::new(*x + 0.055, contact_y + 0.004, z + 0.16),
                        },
                    });
                    floor_joist_interfaces.push(floor_contact);
                    bearing_interfaces.extend([housed, floor_contact]);
                }
            }
        } else {
            // The slab's ground bearing is not a timber joint. Keep it out of
            // the member/joint registry so the structural graph does not
            // invent an empty mortise at the centre of the room.
            let ground_node = StructuralNodeId(builder.next_node);
            builder.next_node += 1;
            builder.geometry.structural_nodes.push(StructuralNode {
                id: ground_node,
                owner,
                kind: StructuralNodeKind::TimberFrameFoundation,
                position: Vec3::new(dimensions.x * 0.5, 0.0, dimensions.y * 0.5),
                supported_by: Vec::new(),
                grounded: true,
            });
            floor_supports.push(ground_node);
            let ground_bearing = ResolvedItemId(
                (4_u64 << 60) | (u64::from(owner.0) << 32) | 0x3b0_000 | builder.next_interface,
            );
            builder.next_interface += 1;
            builder.geometry.support_interfaces.push(SupportInterface {
                id: ground_bearing,
                owner,
                node: ground_node,
                bounds: ResolvedBounds {
                    min: Vec3::new(dimensions.x * 0.5 - 0.25, -0.005, dimensions.y * 0.5 - 0.25),
                    max: Vec3::new(dimensions.x * 0.5 + 0.25, 0.005, dimensions.y * 0.5 + 0.25),
                },
            });
            bearing_interfaces.push(ground_bearing);
        }
        floor_supports.sort_unstable();
        floor_supports.dedup();
        let floor_solid = ResolvedItemId(
            (1_u64 << 60) | (u64::from(owner.0) << 32) | 0x0e00_0000 | u64::from(level + 1),
        );
        let floor_centre_y = base - 0.08;
        let floor_rects = if level == 0 {
            vec![(Vec2::splat(0.15), dimensions - Vec2::splat(0.15))]
        } else {
            vec![
                (
                    Vec2::new(0.15, 0.15),
                    Vec2::new(stair_min.x, dimensions.y - 0.15),
                ),
                (
                    Vec2::new(stair_max.x, 0.15),
                    Vec2::new(dimensions.x - 0.15, dimensions.y - 0.15),
                ),
                (
                    Vec2::new(stair_min.x, 0.15),
                    Vec2::new(stair_max.x, stair_min.y),
                ),
                (
                    Vec2::new(stair_min.x, stair_max.y),
                    Vec2::new(stair_max.x, dimensions.y - 0.15),
                ),
            ]
        };
        let mut floor_solids = Vec::new();
        for (index, (min, max)) in floor_rects.into_iter().enumerate() {
            if (max - min).min_element() <= 0.05 {
                continue;
            }
            let id = if index == 0 {
                floor_solid
            } else {
                ResolvedItemId(floor_solid.0 | ((index as u64) << 12))
            };
            floor_solids.push(id);
            let mut piece_supports = if level == 0 {
                floor_supports.clone()
            } else {
                floor_joist_interfaces
                    .iter()
                    .filter_map(|id| {
                        builder
                            .geometry
                            .support_interfaces
                            .iter()
                            .find(|interface| interface.id == *id)
                    })
                    .filter(|interface| {
                        let centre = (interface.bounds.min + interface.bounds.max) * 0.5;
                        centre.x >= min.x - 0.001
                            && centre.x <= max.x + 0.001
                            && centre.z >= min.y - 0.001
                            && centre.z <= max.y + 0.001
                    })
                    .map(|interface| interface.node)
                    .collect::<Vec<_>>()
            };
            if level > 0 && piece_supports.is_empty() {
                let endpoint = joist_members.iter().find_map(|id| {
                    let member = builder.members.iter().find(|member| member.id == *id)?;
                    [
                        (member.start_node, member.start),
                        (member.end_node, member.end),
                    ]
                    .into_iter()
                    .find(|(_, point)| {
                        point.x >= min.x + 0.01
                            && point.x <= max.x - 0.01
                            && point.z >= min.y + 0.01
                            && point.z <= max.y - 0.01
                    })
                });
                if let Some((node, point)) = endpoint {
                    let contact = ResolvedItemId(
                        (4_u64 << 60)
                            | (u64::from(owner.0) << 32)
                            | 0x3d0_000
                            | builder.next_interface,
                    );
                    builder.next_interface += 1;
                    builder.geometry.support_interfaces.push(SupportInterface {
                        id: contact,
                        owner,
                        node,
                        bounds: ResolvedBounds {
                            min: Vec3::new(point.x - 0.06, base - 0.164, point.z - 0.06),
                            max: Vec3::new(point.x + 0.06, base - 0.156, point.z + 0.06),
                        },
                    });
                    floor_joist_interfaces.push(contact);
                    bearing_interfaces.push(contact);
                    piece_supports.push(node);
                }
            }
            builder.geometry.solids.push(ResolvedSolid {
                id,
                owner,
                centre: Vec3::new((min.x + max.x) * 0.5, floor_centre_y, (min.y + max.y) * 0.5),
                size: Vec3::new(max.x - min.x, 0.16, max.y - min.y),
                yaw_radians: 0.0,
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::FrameFloor,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: piece_supports,
            });
        }
        let route_surface = ResolvedItemId(
            (2_u64 << 60) | (u64::from(owner.0) << 32) | 0x0e00_0000 | u64::from(level + 1),
        );
        builder.geometry.surfaces.push(ResolvedSurface {
            id: route_surface,
            owner,
            bounds: ResolvedBounds {
                min: Vec3::new(0.15, base + 0.001, 0.15),
                max: Vec3::new(dimensions.x - 0.15, base + 0.011, dimensions.y - 0.15),
            },
            role: SurfaceRole::TimberCirculation,
            shape: crate::ResolvedSurfaceShape::Planar,
        });
        floors.push(crate::TimberFloorAssembly {
            level,
            floor_solid,
            floor_solids,
            route_surface,
            girder_members,
            joist_members,
            bearing_interfaces,
            floor_joist_interfaces,
            joist_girder_interfaces,
            stair_connection: (level > 0).then(|| {
                stairs
                    .first()
                    .map_or(
                        Vec2::new(dimensions.x * 0.5, dimensions.y * 0.5),
                        |stair| match *stair {
                            Stair::Straight { start, .. } => start,
                            Stair::Spiral { centre, .. } => centre,
                        },
                    )
            }),
        });
    }

    let entry_opening = openings.iter().find(|opening| {
        opening.use_kind == crate::OpeningUse::Door
            && opening
                .closure
                .layers
                .contains(&crate::ClosureKind::DoorLeaf)
            && opening.sill_elevation_metres <= 0.02
    });
    let mut circulation_nodes = Vec::new();
    let mut circulation_edges = Vec::new();
    let mut circulation_stair_solids = Vec::new();
    let mut circulation_landings = Vec::new();
    let mut floor_cut_voids = Vec::new();
    let mut ground_route_position = Vec2::new(dimensions.x * 0.5, dimensions.y * 0.5);
    let mut previous_surface = floors
        .first()
        .map(|floor| floor.route_surface)
        .expect("timber program has a ground floor");
    if let Some(opening) = entry_opening {
        let approach = ResolvedItemId((2_u64 << 60) | (u64::from(owner.0) << 32) | 0x0d00_0001);
        let threshold = ResolvedItemId((2_u64 << 60) | (u64::from(owner.0) << 32) | 0x0d00_0002);
        let vestibule = ResolvedItemId((2_u64 << 60) | (u64::from(owner.0) << 32) | 0x0d00_0003);
        let approach_centre = opening.frame.origin + opening.frame.outward * 0.75;
        let threshold_centre = opening.frame.origin;
        let vestibule_centre = opening.frame.origin - opening.frame.outward * 0.75;
        ground_route_position = opening.frame.origin - opening.frame.outward * 1.15;
        for (id, centre, depth) in [
            (approach, approach_centre, 0.90_f32),
            (threshold, threshold_centre, 0.35_f32),
            (vestibule, vestibule_centre, 0.90_f32),
        ] {
            builder.geometry.surfaces.push(ResolvedSurface {
                id,
                owner,
                bounds: ResolvedBounds {
                    min: Vec3::new(centre.x - 0.50, 0.001, centre.y - depth * 0.5),
                    max: Vec3::new(centre.x + 0.50, 0.011, centre.y + depth * 0.5),
                },
                role: SurfaceRole::TimberCirculation,
                shape: crate::ResolvedSurfaceShape::Planar,
            });
            circulation_nodes.push(crate::TimberRouteNode {
                surface: id,
                kind: if id == approach {
                    crate::TimberRouteNodeKind::ExteriorApproach
                } else if id == threshold {
                    crate::TimberRouteNodeKind::DoorThreshold
                } else {
                    crate::TimberRouteNodeKind::Landing
                },
                position: Vec3::new(centre.x, 0.01, centre.y),
                level: 0,
            });
        }
        circulation_edges.extend([
            crate::TimberRouteEdge {
                from: approach,
                to: threshold,
                clear_width_metres: 0.90,
                clear_headroom_metres: 2.05,
            },
            crate::TimberRouteEdge {
                from: threshold,
                to: vestibule,
                clear_width_metres: 0.90,
                clear_headroom_metres: 2.05,
            },
            crate::TimberRouteEdge {
                from: vestibule,
                to: previous_surface,
                clear_width_metres: 0.90,
                clear_headroom_metres: 2.05,
            },
        ]);
    }
    circulation_nodes.push(crate::TimberRouteNode {
        surface: previous_surface,
        kind: crate::TimberRouteNodeKind::GroundFloor,
        position: Vec3::new(ground_route_position.x, 0.01, ground_route_position.y),
        level: 0,
    });
    // Route from the entry vestibule to the stair on a quarter-metre lattice,
    // inflating actual internal wall panels by half the 0.90 m occupant width.
    // This avoids the former diagonal shortcut through room partitions while
    // remaining a deliberately compact civilian circulation vocabulary.
    let route_step = 0.05_f32;
    let route_margin = 0.45_f32;
    let nx = ((dimensions.x - route_margin * 2.0) / route_step).floor() as i32;
    let nz = ((dimensions.y - route_margin * 2.0) / route_step).floor() as i32;
    let to_cell = |point: Vec2| {
        (
            ((point.x - route_margin) / route_step)
                .round()
                .clamp(0.0, nx as f32) as i32,
            ((point.y - route_margin) / route_step)
                .round()
                .clamp(0.0, nz as f32) as i32,
        )
    };
    let to_point = |cell: (i32, i32)| {
        Vec2::new(
            route_margin + cell.0 as f32 * route_step,
            route_margin + cell.1 as f32 * route_step,
        )
    };
    let route_start = to_cell(ground_route_position);
    let route_goal = to_cell(stair_origin);
    let blocked = |cell: (i32, i32)| {
        let point = to_point(cell);
        ground_route_wall_bounds.iter().any(|(min, max)| {
            point.x > min.x - route_margin
                && point.x < max.x + route_margin
                && point.y > min.y - route_margin
                && point.y < max.y + route_margin
        })
    };
    let mut frontier = std::collections::VecDeque::from([route_start]);
    let mut came_from = std::collections::HashMap::from([(route_start, route_start)]);
    while let Some(current) = frontier.pop_front() {
        if current == route_goal {
            break;
        }
        for next in [
            (current.0 + 1, current.1),
            (current.0 - 1, current.1),
            (current.0, current.1 + 1),
            (current.0, current.1 - 1),
        ] {
            if next.0 < 0
                || next.1 < 0
                || next.0 > nx
                || next.1 > nz
                || came_from.contains_key(&next)
                || (next != route_goal && next != route_start && blocked(next))
            {
                continue;
            }
            came_from.insert(next, current);
            frontier.push_back(next);
        }
    }
    let mut route_cells = Vec::new();
    if came_from.contains_key(&route_goal) {
        let mut cursor = route_goal;
        route_cells.push(cursor);
        while cursor != route_start {
            cursor = came_from[&cursor];
            route_cells.push(cursor);
        }
        route_cells.reverse();
    }
    let mut route_points = vec![ground_route_position];
    if to_point(route_start).distance(ground_route_position) > 0.03 {
        route_points.push(to_point(route_start));
    }
    for index in 1..route_cells.len() {
        let direction = (
            route_cells[index].0 - route_cells[index - 1].0,
            route_cells[index].1 - route_cells[index - 1].1,
        );
        let next_direction = route_cells
            .get(index + 1)
            .map(|next| (next.0 - route_cells[index].0, next.1 - route_cells[index].1));
        if next_direction != Some(direction) {
            route_points.push(to_point(route_cells[index]));
        }
    }
    if route_points
        .last()
        .is_none_or(|point| point.distance(stair_origin) > 0.03)
    {
        route_points.push(stair_origin);
    }
    for (index, point) in route_points.into_iter().skip(1).enumerate() {
        let surface =
            ResolvedItemId((2_u64 << 60) | (u64::from(owner.0) << 32) | 0x0b00_0000 | index as u64);
        builder.geometry.surfaces.push(ResolvedSurface {
            id: surface,
            owner,
            bounds: ResolvedBounds {
                min: Vec3::new(point.x - 0.45, 0.001, point.y - 0.45),
                max: Vec3::new(point.x + 0.45, 0.011, point.y + 0.45),
            },
            role: SurfaceRole::TimberCirculation,
            shape: crate::ResolvedSurfaceShape::Planar,
        });
        circulation_nodes.push(crate::TimberRouteNode {
            surface,
            kind: crate::TimberRouteNodeKind::Landing,
            position: Vec3::new(point.x, 0.01, point.y),
            level: 0,
        });
        circulation_edges.push(crate::TimberRouteEdge {
            from: previous_surface,
            to: surface,
            clear_width_metres: 0.90,
            clear_headroom_metres: 2.05,
        });
        previous_surface = surface;
    }
    for level in 1..program.storeys.len() as u16 {
        let lower_y = f32::from(level - 1) * program.storey_height_metres;
        let upper_y = f32::from(level) * program.storey_height_metres;
        let tread_count = 18_u64;
        let going = stair_run / tread_count as f32;
        let rise = (upper_y - lower_y) / tread_count as f32;
        let (flight_origin, flight_axis) = if level % 2 == 1 {
            (stair_origin, stair_axis)
        } else {
            (stair_end, -stair_axis)
        };
        let flight_lateral = Vec2::new(-flight_axis.y, flight_axis.x);
        // Split each stringer at every tread bearing. This makes every tread
        // support an actual member endpoint/contact instead of a synthetic
        // node placed near the middle of a diagonal member.
        for side in [-1.0_f32, 1.0] {
            let lateral = flight_lateral * (side * (stair_width * 0.5 - section.x * 0.5));
            for tread in 0..tread_count {
                let start_plan = flight_origin + flight_axis * (going * tread as f32) + lateral;
                let end_plan = flight_origin + flight_axis * (going * (tread + 1) as f32) + lateral;
                builder.member(
                    crate::TimberMemberRole::Girder,
                    Vec3::new(
                        start_plan.x,
                        lower_y + rise * tread as f32 - 0.03,
                        start_plan.y,
                    ),
                    Vec3::new(
                        end_plan.x,
                        lower_y + rise * (tread + 1) as f32 - 0.03,
                        end_plan.y,
                    ),
                    section * 0.90,
                    crate::TimberFramePhase::PrimaryConstruction,
                );
            }
        }
        // The upper floor itself is the eighteenth landing; do not place a
        // duplicate tread inside its subtraction prism.
        for tread in 0..(tread_count - 1) {
            let y = lower_y + rise * (tread + 1) as f32;
            let plan = flight_origin + flight_axis * (going * (tread + 1) as f32);
            let solid_id = ResolvedItemId(
                (1_u64 << 60)
                    | (u64::from(owner.0) << 32)
                    | 0x0c00_0000
                    | (u64::from(level) << 8)
                    | tread,
            );
            let surface_id = ResolvedItemId(
                (2_u64 << 60)
                    | (u64::from(owner.0) << 32)
                    | 0x0c00_0000
                    | (u64::from(level) << 8)
                    | tread,
            );
            let support_nodes = [-1.0_f32, 1.0]
                .map(|side| {
                    builder.node(Vec3::new(
                        plan.x + flight_lateral.x * side * (stair_width * 0.5 - section.x * 0.5),
                        y - 0.03,
                        plan.y + flight_lateral.y * side * (stair_width * 0.5 - section.x * 0.5),
                    ))
                })
                .to_vec();
            for node in &support_nodes {
                let bearing = ResolvedItemId(
                    (4_u64 << 60) | (u64::from(owner.0) << 32) | 0x3c0_000 | builder.next_interface,
                );
                builder.next_interface += 1;
                let node_position = builder
                    .geometry
                    .structural_nodes
                    .iter()
                    .find(|candidate| candidate.id == *node)
                    .expect("stair bearing node exists")
                    .position;
                builder.geometry.support_interfaces.push(SupportInterface {
                    id: bearing,
                    owner,
                    node: *node,
                    bounds: ResolvedBounds {
                        min: node_position - Vec3::new(0.18, 0.025, 0.18),
                        max: node_position + Vec3::new(0.18, 0.035, 0.18),
                    },
                });
            }
            builder.geometry.solids.push(ResolvedSolid {
                id: solid_id,
                owner,
                centre: Vec3::new(plan.x, y - 0.025, plan.y),
                size: Vec3::new(1.0, 0.05, going * 0.96),
                yaw_radians: flight_axis.y.atan2(flight_axis.x) - std::f32::consts::FRAC_PI_2,
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::Landing,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: support_nodes,
            });
            builder.geometry.surfaces.push(ResolvedSurface {
                id: surface_id,
                owner,
                bounds: ResolvedBounds {
                    min: Vec3::new(plan.x - 0.50, y, plan.y - going * 0.48),
                    max: Vec3::new(plan.x + 0.50, y + 0.01, plan.y + going * 0.48),
                },
                role: SurfaceRole::TimberCirculation,
                shape: crate::ResolvedSurfaceShape::Planar,
            });
            circulation_nodes.push(crate::TimberRouteNode {
                surface: surface_id,
                kind: crate::TimberRouteNodeKind::StairTread,
                position: Vec3::new(plan.x, y, plan.y),
                level,
            });
            circulation_edges.push(crate::TimberRouteEdge {
                from: previous_surface,
                to: surface_id,
                clear_width_metres: 1.0,
                clear_headroom_metres: 2.05,
            });
            previous_surface = surface_id;
            circulation_stair_solids.push(solid_id);
        }
        let floor = &floors[usize::from(level)];
        circulation_edges.push(crate::TimberRouteEdge {
            from: previous_surface,
            to: floor.route_surface,
            clear_width_metres: 1.0,
            clear_headroom_metres: 2.05,
        });
        circulation_nodes.push(crate::TimberRouteNode {
            surface: floor.route_surface,
            kind: crate::TimberRouteNodeKind::UpperFloor,
            position: Vec3::new(
                flight_origin.x + flight_axis.x * stair_run,
                upper_y + 0.01,
                flight_origin.y + flight_axis.y * stair_run,
            ),
            level,
        });
        previous_surface = floor.route_surface;
        if let Some(landing) = floor.floor_solids.iter().copied().min_by(|left, right| {
            let score = |id: &ResolvedItemId| {
                builder
                    .geometry
                    .solids
                    .iter()
                    .find(|solid| solid.id == *id)
                    .map_or(f32::INFINITY, |solid| {
                        let half = solid.size * 0.5;
                        let plan = Vec2::new(solid.centre.x, solid.centre.z);
                        let arrival = flight_origin + flight_axis * stair_run;
                        let delta = (arrival - plan).abs() - Vec2::new(half.x, half.z);
                        delta.max(Vec2::ZERO).length() * 100.0 + solid.size.x * solid.size.z
                    })
            };
            score(left).total_cmp(&score(right))
        }) {
            circulation_landings.push(landing);
        }
        let void_id = ResolvedItemId(
            (3_u64 << 60) | (u64::from(owner.0) << 32) | 0x0c00_0000 | u64::from(level),
        );
        let (clear_min, clear_max) = stair_floor_cut(level);
        builder.geometry.voids.push(ResolvedVoid {
            id: void_id,
            owner,
            bounds: ResolvedBounds {
                min: Vec3::new(clear_min.x, upper_y - 0.16, clear_min.y),
                max: Vec3::new(clear_max.x, upper_y - 0.001, clear_max.y),
            },
            role: crate::VoidRole::AccessPortal,
            shape: crate::ResolvedVoidShape::Box,
            subtracts_from: owner,
        });
        floor_cut_voids.push(void_id);
    }
    let circulation = crate::TimberCirculationAssembly {
        entry_opening: entry_opening.map(|opening| opening.id),
        nodes: circulation_nodes,
        edges: circulation_edges,
        stair_solids: circulation_stair_solids,
        landing_solids: circulation_landings,
        floor_cut_voids,
    };

    // Bind dormer curbs and child fronts to the authoritative Stage 4 roof
    // framing / Stage 3 child-wall hosts only where an endpoint interface has
    // positive physical contact. This intentionally replaces the former
    // ground-to-dormer posts, which pierced the parent roof and drainage.
    let endpoint_contacts = builder
        .members
        .iter()
        .filter(|member| {
            member.role == crate::TimberMemberRole::DormerTrimmer
                || bays.iter().any(|bay| {
                    bay.member_ids.contains(&member.id)
                        && bay.wall.is_some_and(|wall_id| {
                            walls.iter().any(|wall| {
                                wall.id == wall_id
                                    && matches!(
                                        wall.source,
                                        crate::WallSourceId::RoofChildFront { .. }
                                    )
                            })
                        })
                })
        })
        .flat_map(|member| {
            [
                (
                    member.start_node,
                    member.support_interfaces[0],
                    member.role == crate::TimberMemberRole::DormerTrimmer,
                ),
                (
                    member.end_node,
                    member.support_interfaces[1],
                    member.role == crate::TimberMemberRole::DormerTrimmer,
                ),
            ]
        })
        .collect::<Vec<_>>();
    for (node_id, interface_id, is_dormer_trimmer) in endpoint_contacts {
        let Some(interface) = builder
            .geometry
            .support_interfaces
            .iter()
            .find(|interface| interface.id == interface_id)
            .cloned()
        else {
            continue;
        };
        let overlaps = |solid: &ResolvedSolid| {
            let half = solid.size * 0.5;
            let min = solid.centre - half;
            let max = solid.centre + half;
            let overlap = interface.bounds.max.min(max) - interface.bounds.min.max(min);
            overlap.cmpgt(Vec3::splat(0.001)).all()
        };
        let mut external_supports = builder
            .geometry
            .solids
            .iter()
            .filter(|solid| {
                matches!(
                    solid.role,
                    SolidRole::RoofFace | SolidRole::RoofFraming | SolidRole::RoofPlate
                ) && overlaps(solid)
            })
            .flat_map(|solid| solid.supported_by.iter().copied())
            .collect::<Vec<_>>();
        if is_dormer_trimmer {
            let node_position = builder
                .geometry
                .structural_nodes
                .iter()
                .find(|node| node.id == node_id)
                .map(|node| node.position);
            if let Some(position) = node_position {
                let plan = Vec2::new(position.x, position.z);
                for parent_roof in roof_assemblies.iter().filter(|roof| roof.parent.is_none()) {
                    let on_parent_plane = parent_roof.faces.iter().any(|face| {
                        let outline = face
                            .polygon
                            .iter()
                            .map(|point| Vec2::new(point.x, point.z))
                            .collect::<Vec<_>>();
                        let inside_face = plan_point_in_polygon(plan, &outline)
                            && !face.cutouts.iter().any(|cutout| {
                                let cutout = cutout
                                    .iter()
                                    .map(|point| Vec2::new(point.x, point.z))
                                    .collect::<Vec<_>>();
                                plan_point_in_polygon(plan, &cutout)
                            });
                        let underside = roof_plane_height(face.plane, plan)
                            - face.plane.normal.normalize_or_zero().y * face.thickness_metres;
                        inside_face && (underside - position.y).abs() <= 0.03
                    });
                    if on_parent_plane {
                        external_supports.extend(parent_roof.support_nodes.iter().copied());
                    }
                }
            }
        }
        for wall in walls.iter().filter(|wall| {
            matches!(wall.source, crate::WallSourceId::RoofChildFront { .. })
                && wall.host_solids.iter().any(|host| {
                    builder
                        .geometry
                        .solids
                        .iter()
                        .find(|solid| solid.id == *host)
                        .is_some_and(&overlaps)
                })
        }) {
            external_supports.push(wall.support_node);
        }
        external_supports.sort_unstable();
        external_supports.dedup();
        if let Some(node) = builder
            .geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == node_id)
        {
            node.supported_by.extend(external_supports);
            node.supported_by.sort_unstable();
            node.supported_by.dedup();
        }
    }

    // Gable frame ends are inset from the exterior wall plates by the roof
    // build-up. Join that known contour offset with short, measured timber
    // seats. This is deliberately bounded to a local perpendicular projection
    // (<= 0.40 m), not the former arbitrary nearest-member search.
    let wall_plates = builder
        .members
        .iter()
        .filter(|member| member.role == crate::TimberMemberRole::WallPlate)
        .cloned()
        .collect::<Vec<_>>();
    let gable_endpoints = builder
        .members
        .iter()
        .filter(|member| member.role == crate::TimberMemberRole::GableTie)
        .flat_map(|member| [member.start, member.end])
        .collect::<Vec<_>>();
    let mut gable_seats = Vec::new();
    for endpoint in gable_endpoints {
        let endpoint_plan = Vec2::new(endpoint.x, endpoint.z);
        let candidate = wall_plates
            .iter()
            .filter_map(|plate| {
                if (plate.start.y - endpoint.y).abs() > 0.02 {
                    return None;
                }
                let start = Vec2::new(plate.start.x, plate.start.z);
                let end = Vec2::new(plate.end.x, plate.end.z);
                let axis = end - start;
                let t = (endpoint_plan - start).dot(axis) / axis.length_squared().max(0.0001);
                if !(-0.001..=1.001).contains(&t) {
                    return None;
                }
                let projected = start + axis * t.clamp(0.0, 1.0);
                let distance = projected.distance(endpoint_plan);
                (distance > 0.051 && distance <= 0.40).then_some((distance, projected))
            })
            .min_by(|left, right| left.0.total_cmp(&right.0));
        if let Some((_, projected)) = candidate {
            gable_seats.push((Vec3::new(projected.x, endpoint.y, projected.y), endpoint));
        }
    }
    for (plate_point, gable_point) in gable_seats {
        builder.member(
            crate::TimberMemberRole::GableTie,
            plate_point,
            gable_point,
            section,
            crate::TimberFramePhase::RoofConstruction,
        );
    }

    builder.resolve_intermediate_member_bearings();
    builder.rebuild_physical_support_tree();

    let mut roof_bearing_interfaces = Vec::new();
    let mut main_roof_supports = roof_assemblies
        .iter()
        .filter(|roof| roof.parent.is_none())
        .flat_map(|roof| &roof.support_nodes)
        .filter_map(|id| {
            builder
                .geometry
                .structural_nodes
                .iter()
                .find(|node| node.id == *id)
                .map(|node| (*id, node.position))
        })
        .filter(|(_, position)| position.y <= top + 0.25)
        .collect::<Vec<_>>();
    main_roof_supports.sort_by(|left, right| {
        left.1
            .x
            .total_cmp(&right.1.x)
            .then(left.1.z.total_cmp(&right.1.z))
    });
    let roof_support_members = builder
        .members
        .iter()
        .filter(|member| {
            matches!(
                member.role,
                crate::TimberMemberRole::WallPlate
                    | crate::TimberMemberRole::Purlin
                    | crate::TimberMemberRole::GableTie
                    | crate::TimberMemberRole::Rafter
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    let roof_seats = main_roof_supports
        .iter()
        .filter_map(|(_, position)| {
            let point = Vec2::new(position.x, position.z);
            roof_support_members
                .iter()
                .filter_map(|plate| {
                    if (plate.start.y - position.y).abs() > 0.02 {
                        return None;
                    }
                    let start = Vec2::new(plate.start.x, plate.start.z);
                    let end = Vec2::new(plate.end.x, plate.end.z);
                    let axis = end - start;
                    let t = (point - start).dot(axis) / axis.length_squared().max(0.0001);
                    if !(-0.001..=1.001).contains(&t) {
                        return None;
                    }
                    let projected = start + axis * t.clamp(0.0, 1.0);
                    let distance = projected.distance(point);
                    // The Stage 4 eave contour may overhang the plate. A
                    // short, facade-perpendicular rafter tail (project gate
                    // <= 0.90 m) supplies that bearing; longer offsets are a
                    // topology error rather than an arbitrary nearest join.
                    (distance > 0.051 && distance <= 0.90).then_some((distance, projected))
                })
                .min_by(|left, right| left.0.total_cmp(&right.0))
                .map(|(_, projected)| (Vec3::new(projected.x, position.y, projected.y), *position))
        })
        .collect::<Vec<_>>();
    for (plate_point, roof_point) in roof_seats {
        builder.member(
            crate::TimberMemberRole::Rafter,
            plate_point,
            roof_point,
            section * 1.10,
            crate::TimberFramePhase::RoofConstruction,
        );
    }
    // Regular truss stations above own the longitudinal ridge/collar purlins.
    // Do not connect this contour list by sort order: consecutive Stage 4
    // support IDs are not necessarily neighbours in the roof topology.
    for (roof_node_id, position) in main_roof_supports {
        let bearing_node = builder.node(position);
        if let Some(node) = builder
            .geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == bearing_node)
        {
            node.kind = StructuralNodeKind::TimberFrameRoofBearing;
        }
        if let Some(roof_node) = builder
            .geometry
            .structural_nodes
            .iter_mut()
            .find(|node| node.id == roof_node_id)
        {
            roof_node.supported_by.push(bearing_node);
            roof_node.supported_by.sort_unstable();
            roof_node.supported_by.dedup();
        }
        let interface = ResolvedItemId(
            (4_u64 << 60) | (u64::from(owner.0) << 32) | 0x200_000 | builder.next_interface,
        );
        builder.next_interface += 1;
        builder.geometry.support_interfaces.push(SupportInterface {
            id: interface,
            owner,
            node: bearing_node,
            bounds: ResolvedBounds {
                min: position - Vec3::new(0.12, 0.08, 0.12),
                max: position + Vec3::new(0.12, 0.08, 0.12),
            },
        });
        roof_bearing_interfaces.push(interface);
    }
    // Roof contour seats may land on the interior of a continuous plate or
    // regular purlin. Resolve those measured point-on-member contacts after
    // the roof nodes exist so none remain synthetic orphan bearings.
    builder.resolve_intermediate_member_bearings();
    builder.rebuild_physical_support_tree();
    // Bind only genuinely intersecting parent/dormer framing solids to those
    // exact seats. A resolved roof item without physical contact is not
    // silently rescued by a nearby frame node.
    let bearing_samples = roof_bearing_interfaces
        .iter()
        .filter_map(|id| {
            builder
                .geometry
                .support_interfaces
                .iter()
                .find(|interface| interface.id == *id)
                .copied()
        })
        .collect::<Vec<_>>();
    let roof_plate_ids = builder
        .geometry
        .solids
        .iter()
        .filter(|solid| matches!(solid.role, SolidRole::RoofPlate | SolidRole::RoofFraming))
        .map(|solid| solid.id)
        .collect::<Vec<_>>();
    for roof_solid_id in roof_plate_ids {
        if let Some(solid) = builder
            .geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == roof_solid_id)
        {
            let half = solid.size * 0.5;
            for sample in &bearing_samples {
                let overlap = sample.bounds.max.min(solid.centre + half)
                    - sample.bounds.min.max(solid.centre - half);
                if overlap.cmpgt(Vec3::splat(0.001)).all() {
                    solid.supported_by.push(sample.node);
                }
            }
            solid.supported_by.sort_unstable();
            solid.supported_by.dedup();
        }
    }

    let mut masonry_bearing_interfaces = Vec::new();
    if program_kind == crate::TimberFrameProgramKind::CivicMasonryTimberHall {
        let sill_contacts = builder
            .members
            .iter()
            .filter(|member| {
                member.role == crate::TimberMemberRole::Sill
                    && (member.start.y - program.storey_height_metres).abs() <= 0.01
            })
            .flat_map(|member| {
                [
                    (member.start_node, member.support_interfaces[0]),
                    (member.end_node, member.support_interfaces[1]),
                ]
            })
            .chain(
                builder
                    .members
                    .iter()
                    .filter(|member| member.role == crate::TimberMemberRole::Knagge)
                    .map(|member| (member.start_node, member.support_interfaces[0])),
            )
            .collect::<Vec<_>>();
        for (node_id, interface_id) in sill_contacts {
            let Some(interface) = builder
                .geometry
                .support_interfaces
                .iter()
                .find(|interface| interface.id == interface_id)
                .copied()
            else {
                continue;
            };
            let masonry_support = walls
                .iter()
                .filter(|wall| {
                    wall.storey_level == 0
                        && wall.material == crate::WallMaterialClass::CivilianMasonry
                })
                .find(|wall| {
                    wall.host_solids.iter().any(|id| {
                        builder
                            .geometry
                            .solids
                            .iter()
                            .find(|solid| solid.id == *id)
                            .is_some_and(|solid| {
                                let half = solid.size * 0.5 + Vec3::splat(0.01);
                                let min = solid.centre - half;
                                let max = solid.centre + half;
                                interface.bounds.max.cmpge(min).all()
                                    && interface.bounds.min.cmple(max).all()
                            })
                    })
                })
                .map(|wall| wall.support_node);
            if let Some(support) = masonry_support
                && let Some(node) = builder
                    .geometry
                    .structural_nodes
                    .iter_mut()
                    .find(|node| node.id == node_id)
            {
                node.supported_by.push(support);
                node.supported_by.sort_unstable();
                node.supported_by.dedup();
                masonry_bearing_interfaces.push(interface_id);
            }
        }
    }

    // Roof-contour members and civic masonry contacts are added after the
    // first floor/frame pass; orient the final physical graph once more so no
    // late member is left with a nominal but ungrounded endpoint.
    builder.resolve_intermediate_member_bearings();
    builder.rebuild_physical_support_tree();
    builder.classify_physical_joints();

    Some(crate::TimberFrameAssembly {
        id: crate::TimberFrameAssemblyId(1),
        program: program_kind,
        phase: crate::TimberFramePhase::PrimaryConstruction,
        material: frame_material,
        facades,
        internal_lines,
        bays,
        members: builder.members,
        joints: builder.joints,
        floors,
        circulation,
        masonry_bearing_interfaces,
        roof_bearing_interfaces,
        dormer_trimmer_members,
    })
}

/// Recomputes an existing roof graph under its declared pivot policy.  A
/// child intersection that would need a new topological cut is rejected
/// explicitly instead of silently detaching the child.
pub fn set_roof_pitch(
    plan: &mut BuildingPlan,
    id: RoofAssemblyId,
    pitch_degrees: f32,
) -> Result<(), RoofEditError> {
    if !(15.0..=75.0).contains(&pitch_degrees) {
        return Err(RoofEditError::PitchOutsideProjectRange);
    }
    let assembly = plan
        .roof_assemblies
        .iter_mut()
        .find(|roof| roof.id == id)
        .ok_or(RoofEditError::MissingAssembly)?;
    if !assembly.children.is_empty() || assembly.parent.is_some() {
        return Err(RoofEditError::TopologyEvent);
    }
    let old_pitch = assembly
        .faces
        .first()
        .map_or(pitch_degrees, |face| face.pitch_degrees);
    if (old_pitch - pitch_degrees).abs() < 0.0001 {
        return Ok(());
    }
    let old_tan = old_pitch.to_radians().tan();
    if old_tan.abs() <= 0.0001 {
        return Err(RoofEditError::TopologyEvent);
    }
    let factor = pitch_degrees.to_radians().tan() / old_tan;
    let min_y = assembly
        .faces
        .iter()
        .flat_map(|face| face.polygon.iter().map(|point| point.y))
        .fold(f32::INFINITY, f32::min);
    let max_y = assembly
        .faces
        .iter()
        .flat_map(|face| face.polygon.iter().map(|point| point.y))
        .fold(f32::NEG_INFINITY, f32::max);
    let scale_y = |y: f32| match assembly.pivot_policy {
        RoofPivotPolicy::KeepEave | RoofPivotPolicy::KeepChildAttachment => {
            min_y + (y - min_y) * factor
        }
        RoofPivotPolicy::KeepRidge => max_y - (max_y - y) * factor,
    };
    for face in &mut assembly.faces {
        for point in &mut face.polygon {
            point.y = scale_y(point.y);
        }
        face.plane = roof_plane(&face.polygon);
        face.pitch_degrees = pitch_degrees;
        let bounds = roof_polygon_bounds(&face.polygon);
        if let Some(surface) = plan
            .resolved_geometry
            .surfaces
            .iter_mut()
            .find(|surface| surface.id == face.drainage_catchment)
        {
            surface.bounds = bounds;
        }
        if let Some(catchment) = plan
            .resolved_geometry
            .drainage_catchments
            .iter_mut()
            .find(|catchment| catchment.id == face.drainage_catchment)
        {
            let centre = face.polygon.iter().copied().sum::<Vec3>() / face.polygon.len() as f32;
            let low = face
                .polygon
                .iter()
                .min_by(|a, b| a.y.total_cmp(&b.y))
                .copied()
                .expect("roof face has vertices");
            catchment.centre = centre;
            catchment.inner_elevation_metres = face
                .polygon
                .iter()
                .map(|point| point.y)
                .fold(f32::NEG_INFINITY, f32::max);
            catchment.outer_elevation_metres = low.y;
            if let Some(route) = plan
                .resolved_geometry
                .drainage_routes
                .iter_mut()
                .find(|route| route.id == catchment.outlet_route)
            {
                route.inlet = centre;
                route.outlet = low;
            }
        }
    }
    for enclosure in &mut assembly.enclosure_faces {
        for point in &mut enclosure.polygon {
            if point.y > min_y + 0.01 {
                point.y = scale_y(point.y);
            }
        }
    }
    for edge in &mut assembly.edges {
        edge.start.y = scale_y(edge.start.y);
        edge.end.y = scale_y(edge.end.y);
    }
    for (edge_index, edge) in assembly.edges.iter().enumerate() {
        let delta = edge.end - edge.start;
        let plan_length = Vec2::new(delta.x, delta.z).length().max(0.05);
        let edge_pitch = delta.y.atan2(plan_length);
        let weather_id =
            ResolvedItemId((0x8_u64 << 60) | (assembly.id.0 << 16) | 0x5000 | edge_index as u64);
        if let Some(solid) = plan
            .resolved_geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == weather_id)
        {
            let treated_plan_length = if edge.kind == RoofEdgeKind::Eave {
                (plan_length - 0.36_f32.min(plan_length * 0.5)).max(0.05)
            } else {
                plan_length
            };
            solid.centre = (edge.start + edge.end) * 0.5
                + if edge.kind == RoofEdgeKind::Eave {
                    Vec3::NEG_Y * 0.06
                } else {
                    Vec3::Y * 0.035
                };
            solid.size.x = if edge.kind == RoofEdgeKind::Eave {
                treated_plan_length
            } else {
                treated_plan_length / edge_pitch.cos().abs().max(0.01)
            };
            solid.yaw_radians = delta.z.atan2(delta.x);
            solid.longfall_radians = if edge.kind == RoofEdgeKind::Eave {
                0.012
            } else {
                edge_pitch
            };
        }
        if let Some(flashing_id) = edge.flashing
            && let Some(flashing) = plan
                .resolved_geometry
                .solids
                .iter_mut()
                .find(|solid| solid.id == flashing_id)
        {
            flashing.centre = (edge.start + edge.end) * 0.5 + Vec3::Y * (flashing.size.y * 0.5);
            flashing.size.x = delta.length().max(0.05);
            flashing.yaw_radians = delta.z.atan2(delta.x);
            flashing.longfall_radians = if edge.kind == RoofEdgeKind::Valley {
                edge_pitch
            } else {
                0.0
            };
        }
    }
    Ok(())
}

fn footprint_cells(footprint: Footprint) -> Result<Vec<Cell>, GenerationError> {
    let (width, depth) = footprint.dimensions();
    if width < 3 || depth < 3 || width > i16::MAX as u16 || depth > i16::MAX as u16 {
        return Err(GenerationError::InvalidFootprint);
    }
    let mut cells = Vec::new();
    match footprint {
        Footprint::Rectangle { .. } => {
            for z in 0..depth {
                for x in 0..width {
                    cells.push(Cell::new(x as i16, z as i16));
                }
            }
        }
        Footprint::Courtyard {
            wing, gate_width, ..
        } => {
            if wing < 2
                || wing * 2 >= width
                || wing * 2 >= depth
                || gate_width == 0
                || gate_width > width - wing * 2
            {
                return Err(GenerationError::InvalidFootprint);
            }
            for z in 0..depth {
                for x in 0..width {
                    if x < wing || x >= width - wing || z < wing || z >= depth - wing {
                        cells.push(Cell::new(x as i16, z as i16));
                    }
                }
            }
        }
    }
    Ok(cells)
}

fn allocate_rooms(
    footprint: &[Cell],
    width: u16,
    depth: u16,
    requirements: &[RoomRequirement],
    seed: u64,
    archetype: BuildingArchetype,
) -> BTreeMap<Cell, usize> {
    let usable = footprint.iter().copied().collect::<BTreeSet<_>>();
    let mut assignments = BTreeMap::new();
    let mut room_seeds = vec![None; requirements.len()];

    if let Some(passage_index) = requirements
        .iter()
        .position(|room| room.kind == RoomKind::Passage)
    {
        let passage_width = match archetype {
            BuildingArchetype::CastleGatehouse => 2,
            BuildingArchetype::CourtyardCastle => 4,
            _ => 1,
        };
        let start_x = i16::try_from(width / 2).unwrap() - passage_width / 2;
        let passage_depth = match archetype {
            BuildingArchetype::CourtyardCastle => match requirements.len() {
                0 => 0,
                _ => 4,
            },
            _ => i16::try_from(depth).unwrap(),
        };
        for z in 0..passage_depth {
            for x in start_x..start_x + passage_width {
                let cell = Cell::new(x, z);
                if usable.contains(&cell) {
                    assignments.insert(cell, passage_index);
                    room_seeds[passage_index].get_or_insert(cell);
                }
            }
        }
    }

    let mut claimed_seeds = assignments.keys().copied().collect::<HashSet<_>>();
    for (room_index, requirement) in requirements.iter().enumerate() {
        if room_seeds[room_index].is_some() {
            continue;
        }
        let selected = footprint
            .iter()
            .copied()
            .filter(|cell| !claimed_seeds.contains(cell))
            .min_by_key(|cell| seed_score(*cell, requirement, width, depth, room_index, seed))
            .expect("room count is bounded by footprint cells");
        assignments.insert(selected, room_index);
        claimed_seeds.insert(selected);
        room_seeds[room_index] = Some(selected);
    }

    while assignments.len() < footprint.len() {
        let room_counts = room_counts(requirements.len(), &assignments);
        let mut best: Option<(u64, u64, u64, Cell, usize)> = None;
        for cell in footprint.iter().copied() {
            if assignments.contains_key(&cell) {
                continue;
            }
            let neighbouring_rooms = Direction::ALL
                .into_iter()
                .filter_map(|direction| assignments.get(&cell.neighbour(direction)).copied())
                .collect::<BTreeSet<_>>();
            for room_index in neighbouring_rooms {
                if requirements[room_index].kind == RoomKind::Passage {
                    continue;
                }
                let preferred = u64::from(requirements[room_index].preferred_cells.max(1));
                let fill_ratio = room_counts[room_index] as u64 * 10_000 / preferred;
                let seed_cell = room_seeds[room_index].expect("every room has a seed");
                let distance =
                    cell.x.abs_diff(seed_cell.x) as u64 + cell.z.abs_diff(seed_cell.z) as u64;
                let same_room_neighbours = Direction::ALL
                    .into_iter()
                    .filter(|direction| {
                        assignments.get(&cell.neighbour(*direction)) == Some(&room_index)
                    })
                    .count() as u64;
                let geometry_score = distance * 8 + (4 - same_room_neighbours) * 12;
                let candidate = (
                    fill_ratio,
                    geometry_score,
                    stable_noise(seed, room_index as u64, cell) % 97,
                    cell,
                    room_index,
                );
                if best.is_none_or(|current| candidate < current) {
                    best = Some(candidate);
                }
            }
        }
        let (_, _, _, cell, room_index) =
            best.expect("connected footprint always has an expansion edge");
        assignments.insert(cell, room_index);
    }

    assignments
}

fn seed_score(
    cell: Cell,
    requirement: &RoomRequirement,
    width: u16,
    depth: u16,
    room_index: usize,
    seed: u64,
) -> u64 {
    let x = i32::from(cell.x);
    let z = i32::from(cell.z);
    let max_x = i32::from(width) - 1;
    let max_z = i32::from(depth) - 1;
    let centre_x = max_x / 2;
    let centre_z = max_z / 2;
    let exterior_distance = x.min(max_x - x).min(z).min(max_z - z).max(0) as u64;
    let centre_distance =
        (x - centre_x).unsigned_abs() as u64 + (z - centre_z).unsigned_abs() as u64;
    let south_centre = z.unsigned_abs() as u64 * 8 + (x - centre_x).unsigned_abs() as u64;
    let north_centre = (max_z - z).unsigned_abs() as u64 * 8 + (x - centre_x).unsigned_abs() as u64;
    let west_centre = x.unsigned_abs() as u64 * 8 + (z - centre_z).unsigned_abs() as u64;
    let east_centre = (max_x - x).unsigned_abs() as u64 * 8 + (z - centre_z).unsigned_abs() as u64;
    let functional = match requirement.kind {
        RoomKind::EntranceHall | RoomKind::Shop | RoomKind::Passage => south_centre,
        RoomKind::StairHall => centre_distance,
        RoomKind::Kitchen | RoomKind::Pantry => north_centre,
        RoomKind::Workshop | RoomKind::Armoury => west_centre,
        RoomKind::Guardroom => east_centre,
        RoomKind::GreatHall
        | RoomKind::CommonRoom
        | RoomKind::Gallery
        | RoomKind::Chapel
        | RoomKind::Nave
        | RoomKind::Chancel => north_centre + centre_distance,
        RoomKind::Storage | RoomKind::Sacristy => west_centre + north_centre,
        RoomKind::Bedchamber | RoomKind::TowerChamber => east_centre + north_centre,
    };
    functional * 1_000
        + if requirement.needs_exterior {
            exterior_distance * 4_000
        } else {
            0
        }
        + stable_noise(seed, room_index as u64, cell) % 499
}

fn stable_noise(seed: u64, salt: u64, cell: Cell) -> u64 {
    let mut value = seed
        ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (cell.x as u16 as u64) << 16
        ^ cell.z as u16 as u64;
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn room_counts(room_count: usize, assignments: &BTreeMap<Cell, usize>) -> Vec<usize> {
    let mut counts = vec![0; room_count];
    for room in assignments.values().copied() {
        counts[room] += 1;
    }
    counts
}

fn collect_rooms(
    assignments: &BTreeMap<Cell, usize>,
    requirements: &[RoomRequirement],
) -> Vec<Room> {
    requirements
        .iter()
        .enumerate()
        .map(|(room_index, requirement)| Room {
            id: room_index as u16,
            kind: requirement.kind,
            cells: assignments
                .iter()
                .filter_map(|(cell, assigned)| (*assigned == room_index).then_some(*cell))
                .collect(),
        })
        .collect()
}

fn cells_are_connected(cells: &[Cell]) -> bool {
    let Some(first) = cells.first().copied() else {
        return false;
    };
    let all = cells.iter().copied().collect::<HashSet<_>>();
    let mut reached = HashSet::from([first]);
    let mut pending = VecDeque::from([first]);
    while let Some(cell) = pending.pop_front() {
        for direction in Direction::ALL {
            let neighbour = cell.neighbour(direction);
            if all.contains(&neighbour) && reached.insert(neighbour) {
                pending.push_back(neighbour);
            }
        }
    }
    reached.len() == cells.len()
}

fn derive_walls(
    footprint: &[Cell],
    assignments: &BTreeMap<Cell, usize>,
) -> Vec<crate::WallSegment> {
    let occupied = footprint.iter().copied().collect::<HashSet<_>>();
    let mut walls = Vec::new();
    for cell in footprint.iter().copied() {
        let inside_room = assignments[&cell] as u16;
        for direction in Direction::ALL {
            let neighbour = cell.neighbour(direction);
            if !occupied.contains(&neighbour) {
                walls.push(crate::WallSegment {
                    cell,
                    direction,
                    inside_room,
                    outside_room: None,
                });
            } else if matches!(direction, Direction::North | Direction::East) {
                let other_room = assignments[&neighbour] as u16;
                if inside_room != other_room {
                    walls.push(crate::WallSegment {
                        cell,
                        direction,
                        inside_room,
                        outside_room: Some(other_room),
                    });
                }
            }
        }
    }
    walls
}

fn derive_openings(
    walls: &[crate::WallSegment],
    requirements: &[RoomRequirement],
    archetype: BuildingArchetype,
    seed: u64,
    level: usize,
) -> Result<Vec<Opening>, GenerationError> {
    let mut openings = Vec::new();
    let exterior_extent = walls
        .iter()
        .filter(|wall| wall.exterior())
        .fold(None, |extent, wall| {
            let cell = wall.cell;
            Some(extent.map_or(
                (cell.x, cell.x, cell.z, cell.z),
                |(min_x, max_x, min_z, max_z): (i16, i16, i16, i16)| {
                    (
                        min_x.min(cell.x),
                        max_x.max(cell.x),
                        min_z.min(cell.z),
                        max_z.max(cell.z),
                    )
                },
            ))
        });
    let mut occupied_walls = HashSet::new();

    if level == 0 {
        let entrance_room = requirements
            .iter()
            .position(|room| matches!(room.kind, RoomKind::EntranceHall | RoomKind::Passage))
            .unwrap_or(0) as u16;
        let mut entrance_candidates = walls
            .iter()
            .enumerate()
            .filter(|(_, wall)| {
                wall.exterior()
                    && wall.inside_room == entrance_room
                    && wall.direction == Direction::South
            })
            .collect::<Vec<_>>();
        entrance_candidates.sort_by_key(|(_, wall)| wall.cell.x);
        let gate = matches!(
            archetype,
            BuildingArchetype::HallHouse
                | BuildingArchetype::CastleGatehouse
                | BuildingArchetype::CourtyardCastle
                | BuildingArchetype::WalledKeep
                | BuildingArchetype::ArtilleryRondelCastle
        );
        let selected_entrances = if gate {
            let middle = entrance_candidates.len() / 2;
            let start = middle.saturating_sub(1);
            &entrance_candidates[start..entrance_candidates.len().min(start + 2)]
        } else {
            let middle = entrance_candidates.len() / 2;
            &entrance_candidates[middle..entrance_candidates.len().min(middle + 1)]
        };
        for (wall_index, _) in selected_entrances {
            openings.push(Opening {
                wall: *wall_index,
                kind: if gate {
                    OpeningKind::Gate
                } else {
                    OpeningKind::Door
                },
                width_metres: if gate { 1.35 } else { 1.0 },
                sill_metres: 0.0,
                height_metres: if gate { 2.8 } else { 2.15 },
            });
            occupied_walls.insert(*wall_index);
        }
        if requirements[usize::from(entrance_room)].kind == RoomKind::Passage {
            let mut exit_candidates = walls
                .iter()
                .enumerate()
                .filter(|(_, wall)| {
                    wall.exterior()
                        && wall.inside_room == entrance_room
                        && wall.direction == Direction::North
                })
                .collect::<Vec<_>>();
            exit_candidates.sort_by_key(|(_, wall)| wall.cell.x);
            let middle = exit_candidates.len() / 2;
            let start = middle.saturating_sub(1);
            for (wall_index, _) in &exit_candidates[start..exit_candidates.len().min(start + 2)] {
                openings.push(Opening {
                    wall: *wall_index,
                    kind: OpeningKind::Gate,
                    width_metres: 1.35,
                    sill_metres: 0.0,
                    height_metres: 2.8,
                });
                occupied_walls.insert(*wall_index);
            }
        }
    }

    let mut shared = BTreeMap::<(u16, u16), Vec<usize>>::new();
    for (wall_index, wall) in walls.iter().enumerate() {
        if let Some(other) = wall.outside_room {
            let pair = if wall.inside_room < other {
                (wall.inside_room, other)
            } else {
                (other, wall.inside_room)
            };
            shared.entry(pair).or_default().push(wall_index);
        }
    }
    let mut edges = shared
        .into_iter()
        .map(|(pair, candidates)| {
            let left = &requirements[usize::from(pair.0)];
            let right = &requirements[usize::from(pair.1)];
            let preferred = left.preferred_neighbours.contains(&right.kind)
                || right.preferred_neighbours.contains(&left.kind);
            (preferred, pair, candidates)
        })
        .collect::<Vec<_>>();
    edges.sort_by_key(|(preferred, pair, _)| (!*preferred, *pair));
    let mut sets = DisjointSets::new(requirements.len());
    for (_, (left, right), candidates) in edges {
        if sets.union(usize::from(left), usize::from(right)) {
            let wall_index = candidates[candidates.len() / 2];
            openings.push(Opening {
                wall: wall_index,
                kind: OpeningKind::Door,
                width_metres: 0.95,
                sill_metres: 0.0,
                height_metres: 2.1,
            });
            occupied_walls.insert(wall_index);
        }
    }
    if sets.component_count() != 1 {
        return Err(GenerationError::DisconnectedStorey { level });
    }

    for (wall_index, wall) in walls.iter().enumerate() {
        if !wall.exterior() || occupied_walls.contains(&wall_index) {
            continue;
        }
        // The two-post HallHouse MVP keeps its roof-carrying transverse
        // frames uninterrupted. The large hall doors remain opening-first;
        // optional ordinary lights are deferred rather than allowing a
        // seed-dependent window to cut a roof brace.
        if archetype == BuildingArchetype::HallHouse {
            continue;
        }
        // A one-cell opening at a perimeter corner consumes the return pier:
        // its jamb/reveal then occupies the perpendicular facade's frame
        // plane. Keep corner cells solid; nearby bays still provide light.
        let corner_cell = exterior_extent.is_some_and(|(min_x, max_x, min_z, max_z)| {
            (wall.cell.x == min_x || wall.cell.x == max_x)
                && (wall.cell.z == min_z || wall.cell.z == max_z)
        });
        if corner_cell {
            continue;
        }
        let room_kind = requirements[usize::from(wall.inside_room)].kind;
        if matches!(
            room_kind,
            RoomKind::Storage | RoomKind::Pantry | RoomKind::Passage
        ) || stable_noise(seed, wall_index as u64, wall.cell).is_multiple_of(3)
        {
            continue;
        }
        let fortified = matches!(
            archetype,
            BuildingArchetype::CastleGatehouse
                | BuildingArchetype::CourtyardCastle
                | BuildingArchetype::WalledKeep
                | BuildingArchetype::ArtilleryRondelCastle
        );
        openings.push(Opening {
            wall: wall_index,
            kind: if fortified {
                OpeningKind::ArrowSlit
            } else {
                OpeningKind::Window
            },
            width_metres: if fortified { 0.18 } else { 0.85 },
            sill_metres: if fortified { 1.2 } else { 0.9 },
            height_metres: if fortified { 0.9 } else { 1.15 },
        });
    }

    openings.sort_by_key(|opening| opening.wall);
    Ok(openings)
}

fn wall_material_and_thickness(
    archetype: BuildingArchetype,
    exterior: bool,
    level: u16,
) -> (crate::WallMaterialClass, crate::WallStructuralRole, f32) {
    if !exterior {
        return if matches!(
            archetype,
            BuildingArchetype::TownHouse
                | BuildingArchetype::HallHouse
                | BuildingArchetype::FachwerkCottage
                | BuildingArchetype::FachwerkMerchantHouse
                | BuildingArchetype::RenaissanceTownHall
        ) {
            (
                crate::WallMaterialClass::InternalTimber,
                crate::WallStructuralRole::LoadBearing,
                0.16,
            )
        } else {
            (
                crate::WallMaterialClass::InternalMasonry,
                crate::WallStructuralRole::LoadBearing,
                0.30,
            )
        };
    }
    match archetype {
        BuildingArchetype::TownHouse
        | BuildingArchetype::HallHouse
        | BuildingArchetype::FachwerkCottage
        | BuildingArchetype::FachwerkMerchantHouse => (
            crate::WallMaterialClass::TimberInfill,
            crate::WallStructuralRole::Infill,
            if level == 0 { 0.24 } else { 0.22 },
        ),
        BuildingArchetype::RenaissanceTownHall if level == 0 => (
            crate::WallMaterialClass::CivilianMasonry,
            crate::WallStructuralRole::LoadBearing,
            0.50,
        ),
        BuildingArchetype::RenaissanceTownHall => (
            crate::WallMaterialClass::TimberInfill,
            crate::WallStructuralRole::Infill,
            0.22,
        ),
        BuildingArchetype::Cathedral => (
            crate::WallMaterialClass::CathedralMasonry,
            crate::WallStructuralRole::Buttressed,
            0.90,
        ),
        BuildingArchetype::CastleGatehouse
        | BuildingArchetype::CourtyardCastle
        | BuildingArchetype::WalledKeep
        | BuildingArchetype::ArtilleryRondelCastle => (
            crate::WallMaterialClass::FortifiedMasonry,
            crate::WallStructuralRole::LoadBearing,
            1.20,
        ),
    }
}

fn two_centred_arc_radius(width_metres: f32, rise_metres: f32) -> f32 {
    let half_span = width_metres * 0.5;
    half_span + (rise_metres * rise_metres - half_span * half_span) / (2.0 * half_span.max(0.01))
}

fn opening_profile_for(
    archetype: BuildingArchetype,
    opening: Opening,
) -> (
    crate::OpeningUse,
    crate::OpeningProfile,
    crate::OpeningHeadKind,
) {
    match opening.kind {
        OpeningKind::Door => (
            crate::OpeningUse::Door,
            crate::OpeningProfile::Rectangular {
                width_metres: if matches!(
                    archetype,
                    BuildingArchetype::CastleGatehouse
                        | BuildingArchetype::CourtyardCastle
                        | BuildingArchetype::WalledKeep
                        | BuildingArchetype::ArtilleryRondelCastle
                ) {
                    // Project gate: a 0.78 m service-door pinch is permitted in
                    // thick inherited masonry where a full 0.90 m route would
                    // erase the bonded corner pier of a single-cell bay.
                    0.78
                } else {
                    opening.width_metres
                },
                height_metres: opening.height_metres,
            },
            if matches!(
                archetype,
                BuildingArchetype::TownHouse
                    | BuildingArchetype::HallHouse
                    | BuildingArchetype::FachwerkCottage
                    | BuildingArchetype::FachwerkMerchantHouse
            ) {
                crate::OpeningHeadKind::TimberLintel
            } else {
                crate::OpeningHeadKind::StoneLintel
            },
        ),
        OpeningKind::Gate => (
            crate::OpeningUse::Gate,
            crate::OpeningProfile::Segmental {
                width_metres: opening.width_metres,
                spring_height_metres: (opening.height_metres - 0.28).max(1.8),
                rise_metres: 0.28,
                intrados_depth_metres: 0.24,
            },
            crate::OpeningHeadKind::SegmentalArch,
        ),
        OpeningKind::Window if archetype == BuildingArchetype::Cathedral => (
            crate::OpeningUse::Window,
            crate::OpeningProfile::PointedTwoCentred {
                width_metres: 1.12,
                spring_height_metres: 3.0,
                apex_height_metres: 4.55,
                arc_radius_metres: two_centred_arc_radius(1.12, 4.55 - 3.0),
            },
            crate::OpeningHeadKind::PointedVoussoir,
        ),
        OpeningKind::Window if archetype == BuildingArchetype::RenaissanceTownHall => (
            crate::OpeningUse::Window,
            crate::OpeningProfile::Segmental {
                width_metres: 0.95,
                spring_height_metres: 1.0,
                rise_metres: 0.28,
                intrados_depth_metres: 0.18,
            },
            crate::OpeningHeadKind::SegmentalArch,
        ),
        OpeningKind::Window => (
            crate::OpeningUse::Window,
            crate::OpeningProfile::Rectangular {
                width_metres: opening.width_metres,
                height_metres: opening.height_metres,
            },
            crate::OpeningHeadKind::TimberLintel,
        ),
        OpeningKind::ArrowSlit
            if matches!(
                archetype,
                BuildingArchetype::WalledKeep | BuildingArchetype::ArtilleryRondelCastle
            ) =>
        {
            (
                crate::OpeningUse::GunLoop,
                crate::OpeningProfile::GunLoop {
                    exterior_width_metres: 0.20,
                    interior_width_metres: 0.92,
                    exterior_height_metres: 0.48,
                    interior_height_metres: 1.10,
                    mount: crate::WeaponMountClass::LightArquebus,
                    traverse_degrees: 28.0,
                    recoil_metres: 0.85,
                    crew_clearance_metres: 1.25,
                },
                crate::OpeningHeadKind::StoneLintel,
            )
        }
        OpeningKind::ArrowSlit => (
            crate::OpeningUse::ArrowLoop,
            crate::OpeningProfile::ArrowLoop {
                exterior_width_metres: 0.14,
                interior_width_metres: 0.68,
                exterior_height_metres: opening.height_metres,
                interior_height_metres: 1.18,
            },
            crate::OpeningHeadKind::StoneLintel,
        ),
    }
}

fn closure_policy_for(
    archetype: BuildingArchetype,
    use_kind: crate::OpeningUse,
) -> crate::ClosurePolicy {
    use crate::{ClosureKind, ClosureState};
    match use_kind {
        crate::OpeningUse::ArrowLoop | crate::OpeningUse::GunLoop => crate::ClosurePolicy {
            layers: vec![ClosureKind::OpenMilitary],
            state: ClosureState::Open,
            thickness_metres: 0.0,
            swing_clearance_metres: 0.0,
        },
        crate::OpeningUse::Door | crate::OpeningUse::Gate => crate::ClosurePolicy {
            layers: vec![ClosureKind::DoorLeaf],
            state: ClosureState::Operable,
            thickness_metres: 0.07,
            swing_clearance_metres: 0.90,
        },
        crate::OpeningUse::Window if archetype == BuildingArchetype::Cathedral => {
            crate::ClosurePolicy {
                layers: vec![ClosureKind::LeadedGlazing],
                state: ClosureState::Closed,
                thickness_metres: 0.025,
                swing_clearance_metres: 0.0,
            }
        }
        crate::OpeningUse::Window => crate::ClosurePolicy {
            layers: vec![
                ClosureKind::TimberShutter,
                if matches!(
                    archetype,
                    BuildingArchetype::FachwerkCottage | BuildingArchetype::HallHouse
                ) {
                    ClosureKind::OiledClothLattice
                } else {
                    ClosureKind::LeadedGlazing
                },
            ],
            state: ClosureState::Operable,
            thickness_metres: 0.045,
            swing_clearance_metres: 0.55,
        },
        crate::OpeningUse::BellOpening => crate::ClosurePolicy {
            layers: vec![ClosureKind::TimberLouvre],
            state: ClosureState::Open,
            thickness_metres: 0.08,
            swing_clearance_metres: 0.0,
        },
    }
}

fn wall_solid(
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    slot: u64,
    centre: Vec3,
    size: Vec3,
    role: SolidRole,
    shape: crate::ResolvedSolidShape,
    support: StructuralNodeId,
) -> ResolvedItemId {
    let id = ResolvedItemId((1_u64 << 60) | (u64::from(owner.0) << 32) | slot);
    geometry.solids.push(ResolvedSolid {
        id,
        owner,
        centre,
        size,
        yaw_radians: 0.0,
        crossfall_radians: 0.0,
        longfall_radians: 0.0,
        role,
        shape,
        supported_by: vec![support],
    });
    geometry.support_interfaces.push(SupportInterface {
        id: ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | slot),
        owner,
        node: support,
        bounds: ResolvedBounds {
            min: Vec3::new(
                centre.x - size.x * 0.5,
                centre.y - size.y * 0.5 - 0.015,
                centre.z - size.z * 0.5,
            ),
            max: Vec3::new(
                centre.x + size.x * 0.5,
                centre.y - size.y * 0.5 + 0.015,
                centre.z + size.z * 0.5,
            ),
        },
    });
    id
}

fn wall_void(
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    slot: u64,
    bounds: ResolvedBounds,
    opening: crate::OpeningAssemblyId,
    exterior_width_metres: f32,
    interior_width_metres: f32,
    exterior_height_metres: f32,
    interior_height_metres: f32,
    exterior_depth_sign: i8,
) -> ResolvedItemId {
    let id = ResolvedItemId((3_u64 << 60) | (u64::from(owner.0) << 32) | slot);
    geometry.voids.push(ResolvedVoid {
        id,
        owner,
        bounds,
        role: VoidRole::WallOpening,
        shape: crate::ResolvedVoidShape::SectionalOpening {
            opening,
            exterior_width_metres,
            interior_width_metres,
            exterior_height_metres,
            interior_height_metres,
            exterior_depth_sign,
        },
        subtracts_from: owner,
    });
    id
}

fn wall_shaped_surface(
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    slot: u64,
    bounds: ResolvedBounds,
    role: SurfaceRole,
    shape: crate::ResolvedSurfaceShape,
) -> ResolvedItemId {
    let id = wall_surface(geometry, owner, slot, bounds, role);
    geometry
        .surfaces
        .iter_mut()
        .find(|surface| surface.id == id)
        .expect("new wall surface")
        .shape = shape;
    id
}

fn wall_surface(
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    slot: u64,
    bounds: ResolvedBounds,
    role: SurfaceRole,
) -> ResolvedItemId {
    let id = ResolvedItemId((9_u64 << 60) | (u64::from(owner.0) << 32) | slot);
    geometry.surfaces.push(ResolvedSurface {
        id,
        owner,
        bounds,
        role,
        shape: crate::ResolvedSurfaceShape::Planar,
    });
    id
}

fn resolve_storey_wall_assemblies(
    program: &BuildingProgram,
    storeys: &[StoreyPlan],
    projected_defenses: &[ProjectedDefenseAssembly],
    geometry: &mut ResolvedGeometry,
) -> (Vec<crate::WallAssembly>, Vec<crate::OpeningAssembly>) {
    let mut walls_out = Vec::new();
    let mut openings_out = Vec::new();
    let mut global_index = 0_u64;
    for storey in storeys {
        let base = f32::from(storey.level) * program.storey_height_metres;
        for (wall_index, wall) in storey.walls.iter().copied().enumerate() {
            let id = crate::WallAssemblyId(global_index + 1);
            let owner = GeometryOwnerId(30_000 + global_index as u32);
            let source = crate::WallSourceId::StoreyWall {
                storey_level: storey.level,
                wall_index,
            };
            let outward = direction_vector(wall.direction);
            let tangent = if wall.is_horizontal() {
                Vec2::X
            } else {
                Vec2::Y
            };
            let projection = if wall.exterior() {
                program.upper_storey_projection_metres * f32::from(storey.level.min(1))
            } else {
                0.0
            };
            let origin = wall.centre() + outward * projection;
            let (material, structural_role, thickness) =
                wall_material_and_thickness(program.archetype, wall.exterior(), storey.level);
            let wall_node = StructuralNodeId(2_000_000 + global_index * 8);
            geometry.structural_nodes.push(StructuralNode {
                id: wall_node,
                owner,
                kind: StructuralNodeKind::WallBearing,
                position: Vec3::new(origin.x, base, origin.y),
                supported_by: Vec::new(),
                grounded: true,
            });
            let replacement = projected_defenses.iter().find(|defense| {
                defense.host_source_walls.iter().any(|candidate| {
                    candidate.storey_level == storey.level && candidate.wall_index == wall_index
                })
            });
            let source_opening = storey
                .openings
                .iter()
                .copied()
                .find(|opening| opening.wall == wall_index);
            let mut host_solids = Vec::new();
            let mut opening_ids = Vec::new();
            if replacement.is_none() {
                if let Some(opening) = source_opening {
                    let opening_id = crate::OpeningAssemblyId(global_index + 1);
                    opening_ids.push(opening_id);
                    let (use_kind, mut profile, head_kind) =
                        opening_profile_for(program.archetype, opening);
                    if use_kind == crate::OpeningUse::Window {
                        let maximum_bay_width = if program.archetype == BuildingArchetype::Cathedral
                        {
                            // Buttressed cathedral bays carry their opening at
                            // the bay divisions; wall thickness is depth, not a
                            // subtraction from the clear facade span.
                            CELL_SIZE_METRES - 0.30
                        } else {
                            (CELL_SIZE_METRES - thickness).max(0.35)
                        };
                        profile = match profile {
                            crate::OpeningProfile::Rectangular {
                                width_metres,
                                height_metres,
                            } => crate::OpeningProfile::Rectangular {
                                width_metres: width_metres.min(maximum_bay_width),
                                height_metres,
                            },
                            crate::OpeningProfile::Segmental {
                                width_metres,
                                spring_height_metres,
                                rise_metres,
                                intrados_depth_metres,
                            } => crate::OpeningProfile::Segmental {
                                width_metres: width_metres.min(maximum_bay_width),
                                spring_height_metres,
                                rise_metres,
                                intrados_depth_metres,
                            },
                            crate::OpeningProfile::PointedTwoCentred {
                                width_metres,
                                spring_height_metres,
                                apex_height_metres,
                                ..
                            } => crate::OpeningProfile::PointedTwoCentred {
                                width_metres: width_metres.min(maximum_bay_width),
                                spring_height_metres,
                                apex_height_metres,
                                arc_radius_metres: two_centred_arc_radius(
                                    width_metres.min(maximum_bay_width),
                                    apex_height_metres - spring_height_metres,
                                ),
                            },
                            other => other,
                        };
                    }
                    let mut mouth_width = profile.interior_width_metres().min(1.30);
                    let mut exterior_width = profile.exterior_width_metres().min(mouth_width);
                    let endpoint_bearing_depth = |sign: f32| {
                        let endpoint = wall.centre() + tangent * sign * CELL_SIZE_METRES * 0.5;
                        storey
                            .walls
                            .iter()
                            .enumerate()
                            .filter_map(|(other_index, other)| {
                                if other_index == wall_index
                                    || wall.is_horizontal() == other.is_horizontal()
                                {
                                    return None;
                                }
                                let other_tangent = if other.is_horizontal() {
                                    Vec2::X
                                } else {
                                    Vec2::Y
                                };
                                [
                                    other.centre() - other_tangent * CELL_SIZE_METRES * 0.5,
                                    other.centre() + other_tangent * CELL_SIZE_METRES * 0.5,
                                ]
                                .into_iter()
                                .any(|candidate| candidate.distance(endpoint) <= 0.02)
                                .then(|| {
                                    wall_material_and_thickness(
                                        program.archetype,
                                        other.exterior(),
                                        storey.level,
                                    )
                                    .2
                                })
                            })
                            .fold(0.0_f32, f32::max)
                    };
                    let negative_bearing = endpoint_bearing_depth(-1.0);
                    let positive_bearing = endpoint_bearing_depth(1.0);
                    let negative_bond = negative_bearing > 0.0;
                    let positive_bond = positive_bearing > 0.0;
                    if program.archetype == BuildingArchetype::Cathedral
                        && use_kind == crate::OpeningUse::Window
                        && (negative_bond || positive_bond)
                    {
                        let corner_clear = if negative_bond && positive_bond {
                            0.58
                        } else {
                            0.84
                        };
                        mouth_width = mouth_width.min(corner_clear);
                        exterior_width = exterior_width.min(mouth_width);
                    }
                    let required_negative = thickness.max(negative_bearing) * 0.5 + 0.03;
                    let required_positive = thickness.max(positive_bearing) * 0.5 + 0.03;
                    if negative_bond && positive_bond {
                        let available =
                            (CELL_SIZE_METRES - required_negative - required_positive).max(0.68);
                        if mouth_width > available {
                            mouth_width = available;
                            exterior_width = exterior_width.min(mouth_width);
                            profile = match profile {
                                crate::OpeningProfile::Rectangular { height_metres, .. } => {
                                    crate::OpeningProfile::Rectangular {
                                        width_metres: mouth_width,
                                        height_metres,
                                    }
                                }
                                crate::OpeningProfile::Segmental {
                                    spring_height_metres,
                                    rise_metres,
                                    intrados_depth_metres,
                                    ..
                                } => crate::OpeningProfile::Segmental {
                                    width_metres: mouth_width,
                                    spring_height_metres,
                                    rise_metres,
                                    intrados_depth_metres,
                                },
                                crate::OpeningProfile::PointedTwoCentred {
                                    spring_height_metres,
                                    apex_height_metres,
                                    ..
                                } => crate::OpeningProfile::PointedTwoCentred {
                                    width_metres: mouth_width,
                                    spring_height_metres,
                                    apex_height_metres,
                                    arc_radius_metres: two_centred_arc_radius(
                                        mouth_width,
                                        apex_height_metres - spring_height_metres,
                                    ),
                                },
                                crate::OpeningProfile::ArrowLoop {
                                    exterior_height_metres,
                                    interior_height_metres,
                                    ..
                                } => crate::OpeningProfile::ArrowLoop {
                                    exterior_width_metres: exterior_width.min(mouth_width - 0.04),
                                    interior_width_metres: mouth_width,
                                    exterior_height_metres,
                                    interior_height_metres,
                                },
                                crate::OpeningProfile::GunLoop {
                                    exterior_height_metres,
                                    interior_height_metres,
                                    mount,
                                    traverse_degrees,
                                    recoil_metres,
                                    crew_clearance_metres,
                                    ..
                                } => crate::OpeningProfile::GunLoop {
                                    exterior_width_metres: exterior_width.min(mouth_width - 0.04),
                                    interior_width_metres: mouth_width,
                                    exterior_height_metres,
                                    interior_height_metres,
                                    mount,
                                    traverse_degrees,
                                    recoil_metres,
                                    crew_clearance_metres,
                                },
                            };
                        }
                    }
                    let nominal_pier = (CELL_SIZE_METRES - mouth_width) * 0.5;
                    let opening_offset = match (negative_bond, positive_bond) {
                        (true, false) => (required_negative - nominal_pier)
                            .max(0.0)
                            .min((nominal_pier - 0.05).max(0.0)),
                        (false, true) => -(required_positive - nominal_pier)
                            .max(0.0)
                            .min((nominal_pier - 0.05).max(0.0)),
                        (true, true) => ((required_negative - required_positive) * 0.5)
                            .clamp(-nominal_pier + 0.05, nominal_pier - 0.05),
                        (false, false) => 0.0,
                    };
                    let origin = origin + tangent * opening_offset;
                    profile = match profile {
                        crate::OpeningProfile::Rectangular { height_metres, .. } => {
                            crate::OpeningProfile::Rectangular {
                                width_metres: mouth_width,
                                height_metres,
                            }
                        }
                        crate::OpeningProfile::Segmental {
                            spring_height_metres,
                            rise_metres,
                            intrados_depth_metres,
                            ..
                        } => crate::OpeningProfile::Segmental {
                            width_metres: mouth_width,
                            spring_height_metres,
                            rise_metres,
                            intrados_depth_metres,
                        },
                        crate::OpeningProfile::PointedTwoCentred {
                            spring_height_metres,
                            apex_height_metres,
                            ..
                        } => crate::OpeningProfile::PointedTwoCentred {
                            width_metres: mouth_width,
                            spring_height_metres,
                            apex_height_metres,
                            arc_radius_metres: two_centred_arc_radius(
                                mouth_width,
                                apex_height_metres - spring_height_metres,
                            ),
                        },
                        crate::OpeningProfile::ArrowLoop {
                            exterior_height_metres,
                            interior_height_metres,
                            ..
                        } => crate::OpeningProfile::ArrowLoop {
                            exterior_width_metres: exterior_width,
                            interior_width_metres: mouth_width,
                            exterior_height_metres,
                            interior_height_metres,
                        },
                        crate::OpeningProfile::GunLoop {
                            exterior_height_metres,
                            interior_height_metres,
                            mount,
                            traverse_degrees,
                            recoil_metres,
                            crew_clearance_metres,
                            ..
                        } => crate::OpeningProfile::GunLoop {
                            exterior_width_metres: exterior_width,
                            interior_width_metres: mouth_width,
                            exterior_height_metres,
                            interior_height_metres,
                            mount,
                            traverse_degrees,
                            recoil_metres,
                            crew_clearance_metres,
                        },
                    };
                    let clear_height = profile
                        .clear_height_metres()
                        .min(program.storey_height_metres - opening.sill_metres - 0.10);
                    let jamb_nodes = [
                        StructuralNodeId(wall_node.0 + 1),
                        StructuralNodeId(wall_node.0 + 2),
                    ];
                    for (side, node) in [-1.0_f32, 1.0].into_iter().zip(jamb_nodes) {
                        geometry.structural_nodes.push(StructuralNode {
                            id: node,
                            owner,
                            kind: StructuralNodeKind::OpeningJamb,
                            position: Vec3::new(
                                origin.x + tangent.x * side * mouth_width * 0.5,
                                base,
                                origin.y + tangent.y * side * mouth_width * 0.5,
                            ),
                            supported_by: vec![wall_node],
                            grounded: false,
                        });
                    }
                    let head_node = StructuralNodeId(wall_node.0 + 3);
                    geometry.structural_nodes.push(StructuralNode {
                        id: head_node,
                        owner,
                        kind: StructuralNodeKind::OpeningHead,
                        position: Vec3::new(
                            origin.x,
                            base + opening.sill_metres + clear_height,
                            origin.y,
                        ),
                        supported_by: jamb_nodes.to_vec(),
                        grounded: false,
                    });
                    let spandrel_node = StructuralNodeId(wall_node.0 + 4);
                    geometry.structural_nodes.push(StructuralNode {
                        id: spandrel_node,
                        owner,
                        kind: StructuralNodeKind::OpeningSpandrel,
                        position: Vec3::new(
                            origin.x,
                            base + program.storey_height_metres,
                            origin.y,
                        ),
                        supported_by: vec![head_node],
                        grounded: false,
                    });
                    let tracery_node =
                        (matches!(profile, crate::OpeningProfile::PointedTwoCentred { .. })
                            && mouth_width >= 0.90)
                            .then(|| {
                                let node = StructuralNodeId(wall_node.0 + 5);
                                geometry.structural_nodes.push(StructuralNode {
                                    id: node,
                                    owner,
                                    kind: StructuralNodeKind::MullionBearing,
                                    position: Vec3::new(
                                        origin.x,
                                        base + opening.sill_metres,
                                        origin.y,
                                    ),
                                    supported_by: vec![wall_node],
                                    grounded: false,
                                });
                                node
                            });
                    // Splayed military apertures are resolved as the actual masonry
                    // wedges between the narrow exterior throat and broad interior
                    // mouth.  The exterior pier footprint is authoritative; its
                    // inner face retreats toward the cell edge through the wall
                    // depth.  A broad rectangular void plus cuboid jambs would leave
                    // the semantic throat disconnected from the rendered opening.
                    let side_widths = [
                        CELL_SIZE_METRES * 0.5 + opening_offset - exterior_width * 0.5,
                        CELL_SIZE_METRES * 0.5 - opening_offset - exterior_width * 0.5,
                    ];
                    let mut jamb_solids = [ResolvedItemId::default(); 2];
                    for (index, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
                        let side_width = side_widths[index];
                        let plan = origin + tangent * side * (exterior_width + side_width) * 0.5;
                        let size = if wall.is_horizontal() {
                            Vec3::new(side_width, program.storey_height_metres, thickness)
                        } else {
                            Vec3::new(thickness, program.storey_height_metres, side_width)
                        };
                        let shape = if mouth_width > exterior_width + 0.01 {
                            crate::ResolvedSolidShape::SplayedReveal {
                                exterior_width_metres: exterior_width,
                                interior_width_metres: mouth_width,
                                side: if side < 0.0 { -1 } else { 1 },
                                exterior_depth_sign: if wall.is_horizontal() {
                                    if outward.y >= 0.0 { 1 } else { -1 }
                                } else if outward.x <= 0.0 {
                                    1
                                } else {
                                    -1
                                },
                            }
                        } else {
                            crate::ResolvedSolidShape::Cuboid
                        };
                        let solid = wall_solid(
                            geometry,
                            owner,
                            index as u64,
                            Vec3::new(plan.x, base + program.storey_height_metres * 0.5, plan.y),
                            size,
                            SolidRole::OpeningJamb,
                            shape,
                            jamb_nodes[index],
                        );
                        jamb_solids[index] = solid;
                        host_solids.push(solid);
                    }
                    let sill_solid = if opening.sill_metres > 0.01 {
                        let size = if wall.is_horizontal() {
                            Vec3::new(mouth_width, opening.sill_metres, thickness)
                        } else {
                            Vec3::new(thickness, opening.sill_metres, mouth_width)
                        };
                        let solid = wall_solid(
                            geometry,
                            owner,
                            2,
                            Vec3::new(origin.x, base + opening.sill_metres * 0.5, origin.y),
                            size,
                            SolidRole::OpeningSill,
                            crate::ResolvedSolidShape::Cuboid,
                            wall_node,
                        );
                        host_solids.push(solid);
                        Some(solid)
                    } else {
                        None
                    };
                    let header_base = opening.sill_metres
                        + match profile {
                            crate::OpeningProfile::Segmental {
                                spring_height_metres,
                                ..
                            }
                            | crate::OpeningProfile::PointedTwoCentred {
                                spring_height_metres,
                                ..
                            } => spring_height_metres,
                            _ => clear_height,
                        };
                    let (head_bottom, head_top, head_shape) = match profile {
                        crate::OpeningProfile::Segmental {
                            width_metres,
                            spring_height_metres,
                            rise_metres,
                            intrados_depth_metres,
                        } => (
                            opening.sill_metres + spring_height_metres,
                            opening.sill_metres + spring_height_metres + rise_metres + 0.20,
                            crate::ResolvedSolidShape::SegmentalArchRing {
                                clear_span_metres: width_metres,
                                spring_height_metres,
                                rise_metres,
                                ring_depth_metres: intrados_depth_metres,
                            },
                        ),
                        crate::OpeningProfile::PointedTwoCentred {
                            width_metres,
                            spring_height_metres,
                            apex_height_metres,
                            arc_radius_metres,
                        } => (
                            opening.sill_metres + spring_height_metres,
                            opening.sill_metres + apex_height_metres + 0.20,
                            crate::ResolvedSolidShape::PointedArchRing {
                                clear_span_metres: width_metres,
                                spring_height_metres,
                                apex_height_metres,
                                arc_radius_metres,
                                ring_depth_metres: 0.20,
                            },
                        ),
                        crate::OpeningProfile::ArrowLoop {
                            exterior_height_metres,
                            interior_height_metres,
                            ..
                        }
                        | crate::OpeningProfile::GunLoop {
                            exterior_height_metres,
                            interior_height_metres,
                            ..
                        } => (
                            opening.sill_metres
                                + exterior_height_metres.min(interior_height_metres),
                            opening.sill_metres
                                + exterior_height_metres.max(interior_height_metres)
                                + 0.20,
                            crate::ResolvedSolidShape::SplayedHead {
                                exterior_clear_height_metres: exterior_height_metres,
                                interior_clear_height_metres: interior_height_metres,
                                exterior_depth_sign: if wall.is_horizontal() {
                                    if outward.y >= 0.0 { 1 } else { -1 }
                                } else if outward.x <= 0.0 {
                                    1
                                } else {
                                    -1
                                },
                            },
                        ),
                        crate::OpeningProfile::Rectangular { .. } => (
                            opening.sill_metres + clear_height,
                            opening.sill_metres + clear_height + 0.20,
                            crate::ResolvedSolidShape::Cuboid,
                        ),
                    };
                    let head_top = head_top.min(program.storey_height_metres - 0.05);
                    let head_height = (head_top - head_bottom).max(0.10);
                    let bearing_width = 0.10_f32.min((CELL_SIZE_METRES - mouth_width) * 0.25);
                    let head_total_width = mouth_width + bearing_width * 2.0;
                    let head_size = if wall.is_horizontal() {
                        Vec3::new(head_total_width, head_height, thickness)
                    } else {
                        Vec3::new(thickness, head_height, head_total_width)
                    };
                    let head_solid = wall_solid(
                        geometry,
                        owner,
                        3,
                        Vec3::new(origin.x, base + head_bottom + head_height * 0.5, origin.y),
                        head_size,
                        SolidRole::OpeningHead,
                        head_shape,
                        head_node,
                    );
                    host_solids.push(head_solid);
                    let spandrel_bottom = (head_top - 0.025).max(head_bottom);
                    let spandrel_height =
                        (program.storey_height_metres - spandrel_bottom).max(0.05);
                    let spandrel_size = if wall.is_horizontal() {
                        Vec3::new(head_total_width, spandrel_height, thickness)
                    } else {
                        Vec3::new(thickness, spandrel_height, head_total_width)
                    };
                    let spandrel_solid = wall_solid(
                        geometry,
                        owner,
                        4,
                        Vec3::new(
                            origin.x,
                            base + spandrel_bottom + spandrel_height * 0.5,
                            origin.y,
                        ),
                        spandrel_size,
                        SolidRole::OpeningSpandrel,
                        crate::ResolvedSolidShape::Cuboid,
                        spandrel_node,
                    );
                    host_solids.push(spandrel_solid);
                    // These interfaces are measured from the resolved head and
                    // pier geometry rather than inferred from node IDs. The
                    // narrow contact bands represent the two springings/end
                    // bearings; the third interface is the measured contact
                    // between this head and a distinct upper-spandrel solid.
                    let head_bearing_interfaces = [-1.0_f32, 1.0].map(|side| {
                        let slot = if side < 0.0 { 50_u64 } else { 51_u64 };
                        let centre_plan =
                            origin + tangent * side * (mouth_width * 0.5 + bearing_width * 0.5);
                        let extent = tangent.abs() * (bearing_width * 0.5)
                            + outward.abs() * (thickness * 0.5);
                        let id = ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | slot);
                        geometry.support_interfaces.push(SupportInterface {
                            id,
                            owner,
                            node: head_node,
                            bounds: ResolvedBounds {
                                min: Vec3::new(
                                    centre_plan.x - extent.x,
                                    base + header_base - 0.025,
                                    centre_plan.y - extent.y,
                                ),
                                max: Vec3::new(
                                    centre_plan.x + extent.x,
                                    base + header_base + 0.025,
                                    centre_plan.y + extent.y,
                                ),
                            },
                        });
                        id
                    });
                    let wall_above_interface =
                        ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | 52);
                    let above_half_tangent = tangent.abs() * (mouth_width * 0.5);
                    let above_half_depth = outward.abs() * (thickness * 0.5);
                    geometry.support_interfaces.push(SupportInterface {
                        id: wall_above_interface,
                        owner,
                        node: spandrel_node,
                        bounds: ResolvedBounds {
                            min: Vec3::new(
                                origin.x - above_half_tangent.x - above_half_depth.x,
                                base + head_top - 0.025,
                                origin.y - above_half_tangent.y - above_half_depth.y,
                            ),
                            max: Vec3::new(
                                origin.x + above_half_tangent.x + above_half_depth.x,
                                base + head_top + 0.025,
                                origin.y + above_half_tangent.y + above_half_depth.y,
                            ),
                        },
                    });
                    let half_tangent = tangent.abs() * (mouth_width * 0.5);
                    let half_depth = outward.abs() * (thickness * 0.55);
                    let (exterior_height, interior_height) = match profile {
                        crate::OpeningProfile::ArrowLoop {
                            exterior_height_metres,
                            interior_height_metres,
                            ..
                        }
                        | crate::OpeningProfile::GunLoop {
                            exterior_height_metres,
                            interior_height_metres,
                            ..
                        } => (
                            exterior_height_metres.min(clear_height),
                            interior_height_metres.min(clear_height),
                        ),
                        _ => (clear_height, clear_height),
                    };
                    let exterior_depth_sign = if wall.is_horizontal() {
                        if outward.y >= 0.0 { 1 } else { -1 }
                    } else if outward.x <= 0.0 {
                        1
                    } else {
                        -1
                    };
                    let void_id = wall_void(
                        geometry,
                        owner,
                        0,
                        ResolvedBounds {
                            min: Vec3::new(
                                origin.x - half_tangent.x - half_depth.x,
                                base + opening.sill_metres,
                                origin.y - half_tangent.y - half_depth.y,
                            ),
                            max: Vec3::new(
                                origin.x + half_tangent.x + half_depth.x,
                                base + opening.sill_metres + clear_height,
                                origin.y + half_tangent.y + half_depth.y,
                            ),
                        },
                        opening_id,
                        exterior_width,
                        mouth_width,
                        exterior_height,
                        interior_height,
                        exterior_depth_sign,
                    );
                    let mut reveal_surfaces = Vec::new();
                    for (index, side) in [-1_i8, 1].into_iter().enumerate() {
                        let along = f32::from(side) * (exterior_width + mouth_width) * 0.25;
                        let plan = origin + tangent * along;
                        let half_depth = outward.abs() * (thickness * 0.5);
                        let half_reveal = tangent.abs() * 0.015;
                        reveal_surfaces.push(wall_shaped_surface(
                            geometry,
                            owner,
                            10 + index as u64,
                            ResolvedBounds {
                                min: Vec3::new(
                                    plan.x - half_depth.x - half_reveal.x,
                                    base + opening.sill_metres,
                                    plan.y - half_depth.y - half_reveal.y,
                                ),
                                max: Vec3::new(
                                    plan.x + half_depth.x + half_reveal.x,
                                    base + opening.sill_metres + clear_height,
                                    plan.y + half_depth.y + half_reveal.y,
                                ),
                            },
                            if side < 0 {
                                SurfaceRole::LeftJambReveal
                            } else {
                                SurfaceRole::RightJambReveal
                            },
                            crate::ResolvedSurfaceShape::SplayedJamb {
                                side,
                                exterior_width_metres: exterior_width,
                                interior_width_metres: mouth_width,
                                exterior_depth_sign,
                            },
                        ));
                    }
                    let half_mouth = tangent.abs() * (mouth_width * 0.5);
                    let half_wall_depth = outward.abs() * (thickness * 0.5);
                    reveal_surfaces.push(wall_shaped_surface(
                        geometry,
                        owner,
                        12,
                        ResolvedBounds {
                            min: Vec3::new(
                                origin.x - half_mouth.x - half_wall_depth.x,
                                base + opening.sill_metres,
                                origin.y - half_mouth.y - half_wall_depth.y,
                            ),
                            max: Vec3::new(
                                origin.x + half_mouth.x + half_wall_depth.x,
                                base + opening.sill_metres + 0.015,
                                origin.y + half_mouth.y + half_wall_depth.y,
                            ),
                        },
                        SurfaceRole::WeatherSill,
                        crate::ResolvedSurfaceShape::WeatherSill {
                            interior_elevation_metres: base + opening.sill_metres,
                            exterior_elevation_metres: base + opening.sill_metres - 0.035,
                            drip_depth_metres: 0.025,
                        },
                    ));
                    let intrados_shape = match profile {
                        crate::OpeningProfile::Segmental {
                            width_metres,
                            spring_height_metres,
                            rise_metres,
                            ..
                        } => crate::ResolvedSurfaceShape::SegmentalIntrados {
                            clear_span_metres: width_metres,
                            spring_height_metres,
                            rise_metres,
                        },
                        crate::OpeningProfile::PointedTwoCentred {
                            width_metres,
                            spring_height_metres,
                            apex_height_metres,
                            arc_radius_metres,
                        } => crate::ResolvedSurfaceShape::PointedIntrados {
                            clear_span_metres: width_metres,
                            spring_height_metres,
                            apex_height_metres,
                            arc_radius_metres,
                        },
                        _ => crate::ResolvedSurfaceShape::Planar,
                    };
                    reveal_surfaces.push(wall_shaped_surface(
                        geometry,
                        owner,
                        13,
                        ResolvedBounds {
                            min: Vec3::new(
                                origin.x - half_mouth.x - half_wall_depth.x,
                                base + header_base - 0.015,
                                origin.y - half_mouth.y - half_wall_depth.y,
                            ),
                            max: Vec3::new(
                                origin.x + half_mouth.x + half_wall_depth.x,
                                base + header_base,
                                origin.y + half_mouth.y + half_wall_depth.y,
                            ),
                        },
                        SurfaceRole::Intrados,
                        intrados_shape,
                    ));
                    for (slot, depth_sign, role, width, height) in [
                        (
                            14_u64,
                            1.0_f32,
                            SurfaceRole::ExteriorThroat,
                            exterior_width,
                            exterior_height,
                        ),
                        (
                            15_u64,
                            -1.0_f32,
                            SurfaceRole::InteriorMouth,
                            mouth_width,
                            interior_height,
                        ),
                    ] {
                        let face = origin + outward * (thickness * 0.5 * depth_sign);
                        let half_width = tangent.abs() * (width * 0.5);
                        let half_face_depth = outward.abs() * 0.006;
                        reveal_surfaces.push(wall_shaped_surface(
                            geometry,
                            owner,
                            slot,
                            ResolvedBounds {
                                min: Vec3::new(
                                    face.x - half_width.x - half_face_depth.x,
                                    base + opening.sill_metres,
                                    face.y - half_width.y - half_face_depth.y,
                                ),
                                max: Vec3::new(
                                    face.x + half_width.x + half_face_depth.x,
                                    base + opening.sill_metres + height,
                                    face.y + half_width.y + half_face_depth.y,
                                ),
                            },
                            role,
                            crate::ResolvedSurfaceShape::Planar,
                        ));
                    }
                    if matches!(profile, crate::OpeningProfile::PointedTwoCentred { .. })
                        && mouth_width >= 0.90
                    {
                        let tracery_node = tracery_node.expect("wide pointed opening tracery node");
                        let mullion_height = match profile {
                            crate::OpeningProfile::PointedTwoCentred {
                                spring_height_metres,
                                ..
                            } => spring_height_metres,
                            _ => clear_height * 0.75,
                        };
                        let bearing_embed = 0.025;
                        let mullion = wall_solid(
                            geometry,
                            owner,
                            12,
                            Vec3::new(
                                origin.x,
                                base + opening.sill_metres - bearing_embed
                                    + (mullion_height + bearing_embed) * 0.5,
                                origin.y,
                            ),
                            if wall.is_horizontal() {
                                Vec3::new(0.08, mullion_height + bearing_embed, thickness * 0.35)
                            } else {
                                Vec3::new(thickness * 0.35, mullion_height + bearing_embed, 0.08)
                            },
                            SolidRole::Mullion,
                            crate::ResolvedSolidShape::Cuboid,
                            tracery_node,
                        );
                        host_solids.push(mullion);
                        let transom = wall_solid(
                            geometry,
                            owner,
                            13,
                            Vec3::new(
                                origin.x,
                                base + opening.sill_metres + mullion_height * 0.72,
                                origin.y,
                            ),
                            if wall.is_horizontal() {
                                Vec3::new(mouth_width * 0.82, 0.09, thickness * 0.30)
                            } else {
                                Vec3::new(thickness * 0.30, 0.09, mouth_width * 0.82)
                            },
                            SolidRole::Mullion,
                            crate::ResolvedSolidShape::Cuboid,
                            tracery_node,
                        );
                        host_solids.push(transom);
                        let extent = tangent.abs() * 0.04 + outward.abs() * (thickness * 0.175);
                        geometry.support_interfaces.push(SupportInterface {
                            id: ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | 53),
                            owner,
                            node: tracery_node,
                            bounds: ResolvedBounds {
                                min: Vec3::new(
                                    origin.x - extent.x,
                                    base + opening.sill_metres - bearing_embed,
                                    origin.y - extent.y,
                                ),
                                max: Vec3::new(
                                    origin.x + extent.x,
                                    base + opening.sill_metres + 0.01,
                                    origin.y + extent.y,
                                ),
                            },
                        });
                    }
                    let closure = closure_policy_for(program.archetype, use_kind);
                    let mut closure_solids = Vec::new();
                    for (index, layer) in closure.layers.iter().copied().enumerate() {
                        if layer == crate::ClosureKind::OpenMilitary {
                            continue;
                        }
                        let plan = origin
                            - outward
                                * (thickness * (0.12 + index as f32 * 0.08)
                                    + if material == crate::WallMaterialClass::TimberInfill {
                                        0.07
                                    } else {
                                        0.0
                                    });
                        if layer == crate::ClosureKind::LeadedGlazing
                            && matches!(profile, crate::OpeningProfile::PointedTwoCentred { .. })
                            && mouth_width >= 0.90
                        {
                            let panel_width = (mouth_width - 0.10) * 0.5;
                            let panel_offset = panel_width * 0.5 + 0.025;
                            let (spring, apex) = match profile {
                                crate::OpeningProfile::PointedTwoCentred {
                                    spring_height_metres,
                                    apex_height_metres,
                                    ..
                                } => (spring_height_metres, apex_height_metres),
                                _ => unreachable!(),
                            };
                            for (panel_index, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
                                let panel_plan = plan + tangent * side * panel_offset;
                                closure_solids.push(wall_solid(
                                    geometry,
                                    owner,
                                    20 + index as u64 * 2 + panel_index as u64,
                                    Vec3::new(
                                        panel_plan.x,
                                        base + opening.sill_metres + clear_height * 0.5,
                                        panel_plan.y,
                                    ),
                                    if wall.is_horizontal() {
                                        Vec3::new(panel_width * 0.94, clear_height, 0.025)
                                    } else {
                                        Vec3::new(0.025, clear_height, panel_width * 0.94)
                                    },
                                    SolidRole::LeadedGlazing,
                                    crate::ResolvedSolidShape::PointedArchRing {
                                        clear_span_metres: panel_width,
                                        spring_height_metres: spring,
                                        apex_height_metres: apex,
                                        arc_radius_metres: two_centred_arc_radius(
                                            panel_width,
                                            apex - spring,
                                        ),
                                        ring_depth_metres: 0.025,
                                    },
                                    tracery_node.expect("wide pointed opening tracery node"),
                                ));
                            }
                            continue;
                        }
                        let role = if layer == crate::ClosureKind::LeadedGlazing {
                            SolidRole::LeadedGlazing
                        } else {
                            SolidRole::OpeningClosure
                        };
                        let closure_shape = match profile {
                            crate::OpeningProfile::Segmental {
                                width_metres,
                                spring_height_metres,
                                rise_metres,
                                intrados_depth_metres,
                            } => crate::ResolvedSolidShape::SegmentalArchRing {
                                clear_span_metres: width_metres,
                                spring_height_metres,
                                rise_metres,
                                ring_depth_metres: intrados_depth_metres,
                            },
                            crate::OpeningProfile::PointedTwoCentred {
                                width_metres,
                                spring_height_metres,
                                apex_height_metres,
                                arc_radius_metres,
                            } => crate::ResolvedSolidShape::PointedArchRing {
                                clear_span_metres: width_metres,
                                spring_height_metres,
                                apex_height_metres,
                                arc_radius_metres,
                                ring_depth_metres: 0.025,
                            },
                            _ => crate::ResolvedSolidShape::Cuboid,
                        };
                        closure_solids.push(wall_solid(
                            geometry,
                            owner,
                            20 + index as u64,
                            Vec3::new(
                                plan.x,
                                base + opening.sill_metres + clear_height * 0.5,
                                plan.y,
                            ),
                            if wall.is_horizontal() {
                                Vec3::new(
                                    (exterior_width * 0.92
                                        - if material == crate::WallMaterialClass::TimberInfill {
                                            0.10
                                        } else {
                                            0.0
                                        })
                                    .max(0.04),
                                    (clear_height * 0.92
                                        - if material == crate::WallMaterialClass::TimberInfill {
                                            0.10
                                        } else {
                                            0.0
                                        })
                                    .max(0.04),
                                    0.025,
                                )
                            } else {
                                Vec3::new(
                                    0.025,
                                    (clear_height * 0.92
                                        - if material == crate::WallMaterialClass::TimberInfill {
                                            0.10
                                        } else {
                                            0.0
                                        })
                                    .max(0.04),
                                    (exterior_width * 0.92
                                        - if material == crate::WallMaterialClass::TimberInfill {
                                            0.10
                                        } else {
                                            0.0
                                        })
                                    .max(0.04),
                                )
                            },
                            role,
                            closure_shape,
                            head_node,
                        ));
                    }
                    let military = matches!(
                        use_kind,
                        crate::OpeningUse::ArrowLoop | crate::OpeningUse::GunLoop
                    );
                    let stance_surface = military.then(|| {
                        projected_surface(
                            geometry,
                            owner,
                            ResolvedBounds {
                                min: Vec3::new(
                                    origin.x - tangent.x.abs() * 0.40 - outward.x.abs() * 0.85,
                                    base,
                                    origin.y - tangent.y.abs() * 0.40 - outward.y.abs() * 0.85,
                                ),
                                max: Vec3::new(
                                    origin.x + tangent.x.abs() * 0.40,
                                    base + 0.02,
                                    origin.y + tangent.y.abs() * 0.40,
                                ),
                            },
                            SurfaceRole::Stance,
                        )
                    });
                    let mount_solid = (use_kind == crate::OpeningUse::GunLoop).then(|| {
                        let plan = origin - outward * thickness * 0.35;
                        wall_solid(
                            geometry,
                            owner,
                            30,
                            Vec3::new(plan.x, base + opening.sill_metres + 0.20, plan.y),
                            Vec3::splat(0.18),
                            SolidRole::WeaponMount,
                            crate::ResolvedSolidShape::Cuboid,
                            wall_node,
                        )
                    });
                    let mut ray_indices = Vec::new();
                    if military {
                        let stance = Vec3::new(
                            origin.x - outward.x * (thickness * 0.5 + 0.55),
                            base,
                            origin.y - outward.y * (thickness * 0.5 + 0.55),
                        );
                        let eye_height = if use_kind == crate::OpeningUse::GunLoop {
                            opening.sill_metres + 0.32
                        } else {
                            opening.sill_metres + clear_height * 0.56
                        };
                        let origin3 = Vec3::new(
                            origin.x - outward.x * (thickness * 0.5 + 0.01),
                            base + eye_height,
                            origin.y - outward.y * (thickness * 0.5 + 0.01),
                        );
                        for (range, distance) in [
                            (ProjectedDefenseRange::Near, 2.0_f32),
                            (ProjectedDefenseRange::Middle, 7.0_f32),
                            (ProjectedDefenseRange::Far, 16.0_f32),
                        ] {
                            ray_indices.push(geometry.projected_defense_rays.len());
                            geometry.projected_defense_rays.push(ProjectedDefenseRay {
                                owner,
                                throat: void_id,
                                stance,
                                origin: origin3,
                                target: origin3
                                    + Vec3::new(
                                        outward.x * distance,
                                        -0.08 * distance.min(5.0),
                                        outward.y * distance,
                                    ),
                                range,
                            });
                        }
                    }
                    openings_out.push(crate::OpeningAssembly {
                        id: opening_id,
                        owner,
                        host_wall: id,
                        host_source: source,
                        frame: crate::WallLocalFrame {
                            origin,
                            tangent,
                            outward,
                            inside_room: Some(wall.inside_room),
                            outside_room: wall.outside_room,
                        },
                        use_kind,
                        profile,
                        sill_elevation_metres: base + opening.sill_metres,
                        closure,
                        head_kind,
                        void_id,
                        jamb_solids,
                        sill_solid,
                        head_solid,
                        spandrel_solid,
                        reveal_surfaces,
                        closure_solids,
                        jamb_nodes,
                        head_node,
                        spandrel_node,
                        tracery_node,
                        stance_surface,
                        mount_solid,
                        ray_indices,
                        sectional_void: (0..=8)
                            .map(|index| {
                                let depth_fraction = index as f32 / 8.0;
                                crate::OpeningVoidSlice {
                                    depth_fraction,
                                    width_metres: exterior_width
                                        + (mouth_width - exterior_width) * depth_fraction,
                                    height_metres: exterior_height
                                        + (interior_height - exterior_height) * depth_fraction,
                                }
                            })
                            .collect(),
                        head_bearing_interfaces,
                        wall_above_interface,
                    });
                } else {
                    // Resolve an ordinary wall bay as two closed tangent
                    // prisms. Section proofs can therefore omit one exact
                    // authority ID and expose a genuine capped cut plane;
                    // the full render remains the exact source envelope.
                    for (slot, side) in [(0_u64, -1.0_f32), (1, 1.0)] {
                        let half_centre = origin + tangent * side * CELL_SIZE_METRES * 0.25;
                        let size = if wall.is_horizontal() {
                            Vec3::new(
                                CELL_SIZE_METRES * 0.5,
                                program.storey_height_metres,
                                thickness,
                            )
                        } else {
                            Vec3::new(
                                thickness,
                                program.storey_height_metres,
                                CELL_SIZE_METRES * 0.5,
                            )
                        };
                        host_solids.push(wall_solid(
                            geometry,
                            owner,
                            slot,
                            Vec3::new(
                                half_centre.x,
                                base + program.storey_height_metres * 0.5,
                                half_centre.y,
                            ),
                            size,
                            SolidRole::WallHost,
                            crate::ResolvedSolidShape::Cuboid,
                            wall_node,
                        ));
                    }
                    if material == crate::WallMaterialClass::CathedralMasonry && wall.exterior() {
                        let buttress_depth = 0.78;
                        for (slot, side) in [(80_u64, -1.0_f32), (81, 1.0)] {
                            let buttress_plan = origin
                                + tangent * side * 0.12
                                + outward * (thickness * 0.5 + buttress_depth * 0.5);
                            host_solids.push(wall_solid(
                                geometry,
                                owner,
                                slot,
                                Vec3::new(
                                    buttress_plan.x,
                                    base + program.storey_height_metres * 0.44,
                                    buttress_plan.y,
                                ),
                                if wall.is_horizontal() {
                                    Vec3::new(
                                        0.24,
                                        program.storey_height_metres * 0.88,
                                        buttress_depth,
                                    )
                                } else {
                                    Vec3::new(
                                        buttress_depth,
                                        program.storey_height_metres * 0.88,
                                        0.24,
                                    )
                                },
                                SolidRole::WallButtress,
                                crate::ResolvedSolidShape::Cuboid,
                                wall_node,
                            ));
                        }
                    }
                }
            }
            walls_out.push(crate::WallAssembly {
                id,
                owner,
                source,
                material,
                storey_level: storey.level,
                frame: crate::WallLocalFrame {
                    origin,
                    tangent,
                    outward,
                    inside_room: Some(wall.inside_room),
                    outside_room: wall.outside_room,
                },
                radial_frame: None,
                length_metres: CELL_SIZE_METRES,
                height_metres: program.storey_height_metres,
                base_elevation_metres: base,
                thickness_metres: thickness,
                structural_role,
                support_node: wall_node,
                host_solids: replacement
                    .map(|defense| defense.host_wall_solids.clone())
                    .unwrap_or(host_solids),
                opening_ids,
                replaced_by_owner: replacement.map(|defense| defense.host_owner),
            });
            global_index += 1;
        }
    }
    (walls_out, openings_out)
}

fn suppress_cathedral_legacy_storey_walls(
    walls: &mut Vec<crate::WallAssembly>,
    openings: &mut Vec<crate::OpeningAssembly>,
    geometry: &mut ResolvedGeometry,
) {
    let removed_owners = walls
        .iter()
        .filter(|wall| matches!(wall.source, crate::WallSourceId::StoreyWall { .. }))
        .map(|wall| wall.owner)
        .collect::<HashSet<_>>();
    walls.retain(|wall| !removed_owners.contains(&wall.owner));
    openings.retain(|opening| !removed_owners.contains(&opening.owner));
    geometry
        .solids
        .retain(|solid| !removed_owners.contains(&solid.owner));
    geometry
        .surfaces
        .retain(|surface| !removed_owners.contains(&surface.owner));
    geometry
        .voids
        .retain(|void| !removed_owners.contains(&void.owner));
    geometry
        .structural_nodes
        .retain(|node| !removed_owners.contains(&node.owner));
    geometry
        .support_interfaces
        .retain(|interface| !removed_owners.contains(&interface.owner));
}

fn resolve_church_tower_door_wall(
    face: Direction,
    opening_id: crate::OpeningAssemblyId,
    wall_id: crate::WallAssemblyId,
    owner: GeometryOwnerId,
    centre: Vec2,
    geometry: &mut ResolvedGeometry,
) -> (crate::WallAssembly, crate::OpeningAssembly) {
    let outward = direction_vector(face);
    let tangent = if outward.y.abs() > 0.5 {
        Vec2::X
    } else {
        Vec2::Y
    };
    let origin = centre + outward * 2.70;
    let length = 4.50_f32;
    let thickness = 0.90_f32;
    let height = 17.30_f32;
    let width = 1.80_f32;
    let clear_height = 3.20_f32;
    let wall_node = StructuralNodeId(7_500_000 + u64::from(owner.0) * 8);
    let jamb_nodes = [
        StructuralNodeId(wall_node.0 + 1),
        StructuralNodeId(wall_node.0 + 2),
    ];
    let head_node = StructuralNodeId(wall_node.0 + 3);
    let spandrel_node = StructuralNodeId(wall_node.0 + 4);
    geometry.structural_nodes.push(StructuralNode {
        id: wall_node,
        owner,
        kind: StructuralNodeKind::WallBearing,
        position: Vec3::new(origin.x, 0.0, origin.y),
        supported_by: Vec::new(),
        grounded: true,
    });
    for (index, node_id) in jamb_nodes.into_iter().enumerate() {
        let side = if index == 0 { -1.0 } else { 1.0 };
        geometry.structural_nodes.push(StructuralNode {
            id: node_id,
            owner,
            kind: StructuralNodeKind::OpeningJamb,
            position: Vec3::new(
                origin.x + tangent.x * side * width * 0.5,
                0.0,
                origin.y + tangent.y * side * width * 0.5,
            ),
            supported_by: vec![wall_node],
            grounded: false,
        });
    }
    geometry.structural_nodes.push(StructuralNode {
        id: head_node,
        owner,
        kind: StructuralNodeKind::OpeningHead,
        position: Vec3::new(origin.x, clear_height, origin.y),
        supported_by: jamb_nodes.to_vec(),
        grounded: false,
    });
    geometry.structural_nodes.push(StructuralNode {
        id: spandrel_node,
        owner,
        kind: StructuralNodeKind::OpeningSpandrel,
        position: Vec3::new(origin.x, clear_height + 0.35, origin.y),
        supported_by: vec![head_node],
        grounded: false,
    });
    let side_width = (length - width) * 0.5;
    let mut jamb_solids = [ResolvedItemId(0); 2];
    let mut host_solids = Vec::new();
    for (index, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
        let position = origin + tangent * side * (width * 0.5 + side_width * 0.5);
        jamb_solids[index] = wall_solid(
            geometry,
            owner,
            index as u64,
            Vec3::new(position.x, height * 0.5, position.y),
            if tangent.x.abs() > 0.5 {
                Vec3::new(side_width, height, thickness)
            } else {
                Vec3::new(thickness, height, side_width)
            },
            SolidRole::OpeningJamb,
            crate::ResolvedSolidShape::Cuboid,
            jamb_nodes[index],
        );
        host_solids.push(jamb_solids[index]);
    }
    let head_solid = wall_solid(
        geometry,
        owner,
        2,
        Vec3::new(origin.x, clear_height + 0.175, origin.y),
        if tangent.x.abs() > 0.5 {
            Vec3::new(width + 0.30, 0.35, thickness)
        } else {
            Vec3::new(thickness, 0.35, width + 0.30)
        },
        SolidRole::OpeningHead,
        crate::ResolvedSolidShape::Cuboid,
        head_node,
    );
    host_solids.push(head_solid);
    let spandrel_bottom = clear_height + 0.325;
    let spandrel_height = height - spandrel_bottom;
    let spandrel_solid = wall_solid(
        geometry,
        owner,
        3,
        Vec3::new(origin.x, spandrel_bottom + spandrel_height * 0.5, origin.y),
        if tangent.x.abs() > 0.5 {
            Vec3::new(width, spandrel_height, thickness)
        } else {
            Vec3::new(thickness, spandrel_height, width)
        },
        SolidRole::OpeningSpandrel,
        crate::ResolvedSolidShape::Cuboid,
        spandrel_node,
    );
    host_solids.push(spandrel_solid);
    let half_tangent = tangent.abs() * (width * 0.5);
    let half_depth = outward.abs() * (thickness * 0.55);
    let depth_sign = if tangent.x.abs() > 0.5 {
        if outward.y >= 0.0 { 1 } else { -1 }
    } else if outward.x <= 0.0 {
        1
    } else {
        -1
    };
    let void_id = wall_void(
        geometry,
        owner,
        0,
        ResolvedBounds {
            min: Vec3::new(
                origin.x - half_tangent.x - half_depth.x,
                0.0,
                origin.y - half_tangent.y - half_depth.y,
            ),
            max: Vec3::new(
                origin.x + half_tangent.x + half_depth.x,
                clear_height,
                origin.y + half_tangent.y + half_depth.y,
            ),
        },
        opening_id,
        width,
        width,
        clear_height,
        clear_height,
        depth_sign,
    );
    let mut reveal_surfaces = Vec::new();
    for (slot, side, role) in [
        (10_u64, -1.0_f32, SurfaceRole::LeftJambReveal),
        (11, 1.0, SurfaceRole::RightJambReveal),
    ] {
        let plan = origin + tangent * side * width * 0.5;
        let extent = outward.abs() * thickness * 0.5 + tangent.abs() * 0.015;
        reveal_surfaces.push(wall_shaped_surface(
            geometry,
            owner,
            slot,
            ResolvedBounds {
                min: Vec3::new(plan.x - extent.x, 0.0, plan.y - extent.y),
                max: Vec3::new(plan.x + extent.x, clear_height, plan.y + extent.y),
            },
            role,
            crate::ResolvedSurfaceShape::SplayedJamb {
                side: side as i8,
                exterior_width_metres: width,
                interior_width_metres: width,
                exterior_depth_sign: depth_sign,
            },
        ));
    }
    reveal_surfaces.push(wall_shaped_surface(
        geometry,
        owner,
        12,
        ResolvedBounds {
            min: Vec3::new(
                origin.x - half_tangent.x - half_depth.x,
                clear_height - 0.02,
                origin.y - half_tangent.y - half_depth.y,
            ),
            max: Vec3::new(
                origin.x + half_tangent.x + half_depth.x,
                clear_height + 0.02,
                origin.y + half_tangent.y + half_depth.y,
            ),
        },
        SurfaceRole::Intrados,
        crate::ResolvedSurfaceShape::Planar,
    ));
    reveal_surfaces.push(wall_shaped_surface(
        geometry,
        owner,
        15,
        ResolvedBounds {
            min: Vec3::new(
                origin.x - half_tangent.x - half_depth.x,
                -0.025,
                origin.y - half_tangent.y - half_depth.y,
            ),
            max: Vec3::new(
                origin.x + half_tangent.x + half_depth.x,
                0.025,
                origin.y + half_tangent.y + half_depth.y,
            ),
        },
        SurfaceRole::WeatherSill,
        crate::ResolvedSurfaceShape::WeatherSill {
            interior_elevation_metres: 0.02,
            exterior_elevation_metres: -0.02,
            drip_depth_metres: 0.025,
        },
    ));
    for (slot, sign, role) in [
        (13_u64, 1.0_f32, SurfaceRole::ExteriorThroat),
        (14, -1.0, SurfaceRole::InteriorMouth),
    ] {
        let plan = origin + outward * thickness * 0.5 * sign;
        reveal_surfaces.push(wall_shaped_surface(
            geometry,
            owner,
            slot,
            ResolvedBounds {
                min: Vec3::new(
                    plan.x - half_tangent.x - 0.006,
                    0.0,
                    plan.y - half_tangent.y - 0.006,
                ),
                max: Vec3::new(
                    plan.x + half_tangent.x + 0.006,
                    clear_height,
                    plan.y + half_tangent.y + 0.006,
                ),
            },
            role,
            crate::ResolvedSurfaceShape::Planar,
        ));
    }
    let leaf_plan = origin - outward * thickness * 0.20;
    let closure_solid = wall_solid(
        geometry,
        owner,
        20,
        Vec3::new(leaf_plan.x, clear_height * 0.5, leaf_plan.y),
        if tangent.x.abs() > 0.5 {
            Vec3::new(width * 0.94, clear_height * 0.96, 0.06)
        } else {
            Vec3::new(0.06, clear_height * 0.96, width * 0.94)
        },
        SolidRole::OpeningClosure,
        crate::ResolvedSolidShape::Cuboid,
        jamb_nodes[0],
    );
    let bearing_width = 0.15_f32;
    let head_bearing_interfaces = [-1.0_f32, 1.0].map(|side| {
        let slot = if side < 0.0 { 50 } else { 51 };
        let plan = origin + tangent * side * (width * 0.5 + bearing_width * 0.5);
        let extent = tangent.abs() * bearing_width * 0.5 + outward.abs() * thickness * 0.5;
        let id = ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | slot);
        geometry.support_interfaces.push(SupportInterface {
            id,
            owner,
            node: head_node,
            bounds: ResolvedBounds {
                min: Vec3::new(plan.x - extent.x, clear_height - 0.025, plan.y - extent.y),
                max: Vec3::new(plan.x + extent.x, clear_height + 0.025, plan.y + extent.y),
            },
        });
        id
    });
    let wall_above_interface = ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | 52);
    geometry.support_interfaces.push(SupportInterface {
        id: wall_above_interface,
        owner,
        node: spandrel_node,
        bounds: ResolvedBounds {
            min: Vec3::new(
                origin.x - half_tangent.x - half_depth.x,
                clear_height + 0.325,
                origin.y - half_tangent.y - half_depth.y,
            ),
            max: Vec3::new(
                origin.x + half_tangent.x + half_depth.x,
                clear_height + 0.375,
                origin.y + half_tangent.y + half_depth.y,
            ),
        },
    });
    let source = crate::WallSourceId::ChurchTowerFace {
        face,
        stage: crate::ChurchTowerStage::Portal,
        bay: 0,
    };
    let wall = crate::WallAssembly {
        id: wall_id,
        owner,
        source,
        material: crate::WallMaterialClass::CathedralMasonry,
        storey_level: 0,
        frame: crate::WallLocalFrame {
            origin,
            tangent,
            outward,
            inside_room: None,
            outside_room: None,
        },
        radial_frame: None,
        length_metres: length,
        height_metres: height,
        base_elevation_metres: 0.0,
        thickness_metres: thickness,
        structural_role: crate::WallStructuralRole::LoadBearing,
        support_node: wall_node,
        host_solids,
        opening_ids: vec![opening_id],
        replaced_by_owner: None,
    };
    let opening = crate::OpeningAssembly {
        id: opening_id,
        owner,
        host_wall: wall_id,
        host_source: source,
        frame: wall.frame,
        use_kind: crate::OpeningUse::Door,
        profile: crate::OpeningProfile::Rectangular {
            width_metres: width,
            height_metres: clear_height,
        },
        sill_elevation_metres: 0.0,
        closure: crate::ClosurePolicy {
            layers: vec![crate::ClosureKind::DoorLeaf],
            state: crate::ClosureState::Operable,
            thickness_metres: 0.06,
            swing_clearance_metres: 1.0,
        },
        head_kind: crate::OpeningHeadKind::StoneLintel,
        void_id,
        jamb_solids,
        sill_solid: None,
        head_solid,
        spandrel_solid,
        reveal_surfaces,
        closure_solids: vec![closure_solid],
        jamb_nodes,
        head_node,
        spandrel_node,
        tracery_node: None,
        stance_surface: None,
        mount_solid: None,
        ray_indices: Vec::new(),
        sectional_void: (0..=8)
            .map(|index| crate::OpeningVoidSlice {
                depth_fraction: index as f32 / 8.0,
                width_metres: width,
                height_metres: clear_height,
            })
            .collect(),
        head_bearing_interfaces,
        wall_above_interface,
    };
    (wall, opening)
}

fn resolve_cathedral_bell_stage(
    towers: &[SquareTower],
    walls: &mut Vec<crate::WallAssembly>,
    openings: &mut Vec<crate::OpeningAssembly>,
    geometry: &mut ResolvedGeometry,
) {
    for (tower_index, tower) in towers
        .iter()
        .enumerate()
        .filter(|(_, tower)| tower.bell_openings)
    {
        let stage_height = 4.2_f32;
        let base = tower.wall_height_metres - stage_height;
        let thickness = 0.90_f32;
        for (face_index, face) in [
            Direction::North,
            Direction::East,
            Direction::South,
            Direction::West,
        ]
        .into_iter()
        .enumerate()
        {
            let outward = direction_vector(face);
            let tangent = if outward.y.abs() > 0.5 {
                Vec2::X
            } else {
                Vec2::Y
            };
            let face_span = if tangent.x.abs() > 0.5 {
                tower.size.x
            } else {
                tower.size.y
            };
            let depth_span = if outward.x.abs() > 0.5 {
                tower.size.x
            } else {
                tower.size.y
            };
            let bay_length = face_span * 0.5;
            for bay in 0..2_u8 {
                let serial = (tower_index * 8 + face_index * 2 + usize::from(bay)) as u64;
                let wall_id = crate::WallAssemblyId(50_000 + serial);
                let opening_id = crate::OpeningAssemblyId(50_000 + serial);
                let owner = GeometryOwnerId(45_000 + serial as u32);
                let wall_node = StructuralNodeId(3_000_000 + serial * 8);
                let bay_sign = if bay == 0 { -1.0 } else { 1.0 };
                let origin = tower.centre
                    + outward * (depth_span * 0.5)
                    + tangent * (bay_sign * bay_length * 0.5);
                geometry.structural_nodes.push(StructuralNode {
                    id: wall_node,
                    owner,
                    kind: StructuralNodeKind::WallBearing,
                    position: Vec3::new(origin.x, 0.0, origin.y),
                    supported_by: Vec::new(),
                    grounded: true,
                });
                let width = 1.15_f32;
                let sill = 0.45_f32;
                let spring = 2.10_f32;
                let apex = 3.35_f32;
                let radius = two_centred_arc_radius(width, apex - spring);
                let profile = crate::OpeningProfile::PointedTwoCentred {
                    width_metres: width,
                    spring_height_metres: spring,
                    apex_height_metres: apex,
                    arc_radius_metres: radius,
                };
                let jamb_nodes = [
                    StructuralNodeId(wall_node.0 + 1),
                    StructuralNodeId(wall_node.0 + 2),
                ];
                for (side, node) in [-1.0_f32, 1.0].into_iter().zip(jamb_nodes) {
                    geometry.structural_nodes.push(StructuralNode {
                        id: node,
                        owner,
                        kind: StructuralNodeKind::OpeningJamb,
                        position: Vec3::new(
                            origin.x + tangent.x * side * width * 0.5,
                            base,
                            origin.y + tangent.y * side * width * 0.5,
                        ),
                        supported_by: vec![wall_node],
                        grounded: false,
                    });
                }
                let head_node = StructuralNodeId(wall_node.0 + 3);
                geometry.structural_nodes.push(StructuralNode {
                    id: head_node,
                    owner,
                    kind: StructuralNodeKind::OpeningHead,
                    position: Vec3::new(origin.x, base + sill + apex, origin.y),
                    supported_by: jamb_nodes.to_vec(),
                    grounded: false,
                });
                let spandrel_node = StructuralNodeId(wall_node.0 + 4);
                geometry.structural_nodes.push(StructuralNode {
                    id: spandrel_node,
                    owner,
                    kind: StructuralNodeKind::OpeningSpandrel,
                    position: Vec3::new(origin.x, base + stage_height, origin.y),
                    supported_by: vec![head_node],
                    grounded: false,
                });
                let side_width = (bay_length - width) * 0.5;
                let mut jamb_solids = [ResolvedItemId::default(); 2];
                let mut host_solids = Vec::new();
                // The tower/nave weather junction lies below the bell stage.
                // Resolve that upper shaft as part of the same bay authority;
                // the lower eight metres remain the existing monolithic tower
                // base and do not need opening subdivision.
                let shaft_base = 8.0_f32;
                let shaft_height = base - shaft_base;
                let shaft_solid = wall_solid(
                    geometry,
                    owner,
                    60,
                    Vec3::new(origin.x, shaft_base + shaft_height * 0.5, origin.y),
                    if tangent.x.abs() > 0.5 {
                        Vec3::new(bay_length, shaft_height, thickness)
                    } else {
                        Vec3::new(thickness, shaft_height, bay_length)
                    },
                    SolidRole::WallHost,
                    crate::ResolvedSolidShape::Cuboid,
                    wall_node,
                );
                host_solids.push(shaft_solid);
                for (index, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
                    let plan = origin + tangent * side * (width + side_width) * 0.5;
                    let id = wall_solid(
                        geometry,
                        owner,
                        index as u64,
                        Vec3::new(plan.x, base + stage_height * 0.5, plan.y),
                        if tangent.x.abs() > 0.5 {
                            Vec3::new(side_width, stage_height, thickness)
                        } else {
                            Vec3::new(thickness, stage_height, side_width)
                        },
                        SolidRole::OpeningJamb,
                        crate::ResolvedSolidShape::Cuboid,
                        jamb_nodes[index],
                    );
                    jamb_solids[index] = id;
                    host_solids.push(id);
                }
                let sill_solid = wall_solid(
                    geometry,
                    owner,
                    2,
                    Vec3::new(origin.x, base + sill * 0.5, origin.y),
                    if tangent.x.abs() > 0.5 {
                        Vec3::new(width, sill, thickness)
                    } else {
                        Vec3::new(thickness, sill, width)
                    },
                    SolidRole::OpeningSill,
                    crate::ResolvedSolidShape::Cuboid,
                    wall_node,
                );
                host_solids.push(sill_solid);
                let bearing_width = 0.12_f32;
                let header_base = sill + spring;
                let head_top = sill + apex + 0.20;
                let header_height = head_top - header_base;
                let head_solid = wall_solid(
                    geometry,
                    owner,
                    3,
                    Vec3::new(origin.x, base + header_base + header_height * 0.5, origin.y),
                    if tangent.x.abs() > 0.5 {
                        Vec3::new(width + bearing_width * 2.0, header_height, thickness)
                    } else {
                        Vec3::new(thickness, header_height, width + bearing_width * 2.0)
                    },
                    SolidRole::OpeningHead,
                    crate::ResolvedSolidShape::PointedArchRing {
                        clear_span_metres: width,
                        spring_height_metres: spring,
                        apex_height_metres: apex,
                        arc_radius_metres: radius,
                        ring_depth_metres: 0.22,
                    },
                    head_node,
                );
                host_solids.push(head_solid);
                let spandrel_bottom = head_top - 0.025;
                let spandrel_height = stage_height - spandrel_bottom;
                let spandrel_solid = wall_solid(
                    geometry,
                    owner,
                    4,
                    Vec3::new(
                        origin.x,
                        base + spandrel_bottom + spandrel_height * 0.5,
                        origin.y,
                    ),
                    if tangent.x.abs() > 0.5 {
                        Vec3::new(width + bearing_width * 2.0, spandrel_height, thickness)
                    } else {
                        Vec3::new(thickness, spandrel_height, width + bearing_width * 2.0)
                    },
                    SolidRole::OpeningSpandrel,
                    crate::ResolvedSolidShape::Cuboid,
                    spandrel_node,
                );
                host_solids.push(spandrel_solid);
                let half_tangent = tangent.abs() * (width * 0.5);
                let half_depth = outward.abs() * (thickness * 0.55);
                let depth_sign = if tangent.x.abs() > 0.5 {
                    if outward.y >= 0.0 { 1 } else { -1 }
                } else if outward.x <= 0.0 {
                    1
                } else {
                    -1
                };
                let void_id = wall_void(
                    geometry,
                    owner,
                    0,
                    ResolvedBounds {
                        min: Vec3::new(
                            origin.x - half_tangent.x - half_depth.x,
                            base + sill,
                            origin.y - half_tangent.y - half_depth.y,
                        ),
                        max: Vec3::new(
                            origin.x + half_tangent.x + half_depth.x,
                            base + sill + apex,
                            origin.y + half_tangent.y + half_depth.y,
                        ),
                    },
                    opening_id,
                    width,
                    width,
                    apex,
                    apex,
                    depth_sign,
                );
                let mut reveal_surfaces = Vec::new();
                for (slot, side, role) in [
                    (10_u64, -1_i8, SurfaceRole::LeftJambReveal),
                    (11, 1, SurfaceRole::RightJambReveal),
                ] {
                    let plan = origin + tangent * (f32::from(side) * width * 0.5);
                    let hd = outward.abs() * (thickness * 0.5);
                    let hr = tangent.abs() * 0.015;
                    reveal_surfaces.push(wall_shaped_surface(
                        geometry,
                        owner,
                        slot,
                        ResolvedBounds {
                            min: Vec3::new(plan.x - hd.x - hr.x, base + sill, plan.y - hd.y - hr.y),
                            max: Vec3::new(
                                plan.x + hd.x + hr.x,
                                base + sill + apex,
                                plan.y + hd.y + hr.y,
                            ),
                        },
                        role,
                        crate::ResolvedSurfaceShape::SplayedJamb {
                            side,
                            exterior_width_metres: width,
                            interior_width_metres: width,
                            exterior_depth_sign: depth_sign,
                        },
                    ));
                }
                reveal_surfaces.push(wall_shaped_surface(
                    geometry,
                    owner,
                    12,
                    ResolvedBounds {
                        min: Vec3::new(
                            origin.x - half_tangent.x - half_depth.x,
                            base + sill - 0.035,
                            origin.y - half_tangent.y - half_depth.y,
                        ),
                        max: Vec3::new(
                            origin.x + half_tangent.x + half_depth.x,
                            base + sill + 0.015,
                            origin.y + half_tangent.y + half_depth.y,
                        ),
                    },
                    SurfaceRole::WeatherSill,
                    crate::ResolvedSurfaceShape::WeatherSill {
                        interior_elevation_metres: base + sill,
                        exterior_elevation_metres: base + sill - 0.035,
                        drip_depth_metres: 0.025,
                    },
                ));
                reveal_surfaces.push(wall_shaped_surface(
                    geometry,
                    owner,
                    13,
                    ResolvedBounds {
                        min: Vec3::new(
                            origin.x - half_tangent.x - half_depth.x,
                            base + sill + spring - 0.015,
                            origin.y - half_tangent.y - half_depth.y,
                        ),
                        max: Vec3::new(
                            origin.x + half_tangent.x + half_depth.x,
                            base + sill + apex,
                            origin.y + half_tangent.y + half_depth.y,
                        ),
                    },
                    SurfaceRole::Intrados,
                    crate::ResolvedSurfaceShape::PointedIntrados {
                        clear_span_metres: width,
                        spring_height_metres: spring,
                        apex_height_metres: apex,
                        arc_radius_metres: radius,
                    },
                ));
                for (slot, sign, role) in [
                    (14_u64, 1.0_f32, SurfaceRole::ExteriorThroat),
                    (15, -1.0, SurfaceRole::InteriorMouth),
                ] {
                    let face_plan = origin + outward * (thickness * 0.5 * sign);
                    let hf = outward.abs() * 0.006;
                    reveal_surfaces.push(wall_shaped_surface(
                        geometry,
                        owner,
                        slot,
                        ResolvedBounds {
                            min: Vec3::new(
                                face_plan.x - half_tangent.x - hf.x,
                                base + sill,
                                face_plan.y - half_tangent.y - hf.y,
                            ),
                            max: Vec3::new(
                                face_plan.x + half_tangent.x + hf.x,
                                base + sill + apex,
                                face_plan.y + half_tangent.y + hf.y,
                            ),
                        },
                        role,
                        crate::ResolvedSurfaceShape::Planar,
                    ));
                }
                let mut closure_solids = Vec::new();
                for (index, height) in [0.75_f32, 1.25, 1.75, 2.25].into_iter().enumerate() {
                    closure_solids.push(wall_solid(
                        geometry,
                        owner,
                        20 + index as u64,
                        Vec3::new(
                            origin.x - outward.x * thickness * 0.20,
                            base + sill + height,
                            origin.y - outward.y * thickness * 0.20,
                        ),
                        if tangent.x.abs() > 0.5 {
                            Vec3::new(width * 0.82, 0.10, 0.09)
                        } else {
                            Vec3::new(0.09, 0.10, width * 0.82)
                        },
                        SolidRole::OpeningClosure,
                        crate::ResolvedSolidShape::Cuboid,
                        head_node,
                    ));
                }
                let head_bearing_interfaces = [-1.0_f32, 1.0].map(|side| {
                    let slot = if side < 0.0 { 50 } else { 51 };
                    let centre_plan = origin + tangent * side * (width * 0.5 + bearing_width * 0.5);
                    let extent =
                        tangent.abs() * (bearing_width * 0.5) + outward.abs() * (thickness * 0.5);
                    let id = ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | slot);
                    geometry.support_interfaces.push(SupportInterface {
                        id,
                        owner,
                        node: head_node,
                        bounds: ResolvedBounds {
                            min: Vec3::new(
                                centre_plan.x - extent.x,
                                base + header_base - 0.025,
                                centre_plan.y - extent.y,
                            ),
                            max: Vec3::new(
                                centre_plan.x + extent.x,
                                base + header_base + 0.025,
                                centre_plan.y + extent.y,
                            ),
                        },
                    });
                    id
                });
                let wall_above_interface =
                    ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | 52);
                geometry.support_interfaces.push(SupportInterface {
                    id: wall_above_interface,
                    owner,
                    node: spandrel_node,
                    bounds: ResolvedBounds {
                        min: Vec3::new(
                            origin.x - half_tangent.x - outward.x.abs() * thickness * 0.5,
                            base + head_top - 0.025,
                            origin.y - half_tangent.y - outward.y.abs() * thickness * 0.5,
                        ),
                        max: Vec3::new(
                            origin.x + half_tangent.x + outward.x.abs() * thickness * 0.5,
                            base + head_top + 0.025,
                            origin.y + half_tangent.y + outward.y.abs() * thickness * 0.5,
                        ),
                    },
                });
                openings.push(crate::OpeningAssembly {
                    id: opening_id,
                    owner,
                    host_wall: wall_id,
                    host_source: crate::WallSourceId::SquareTowerFace {
                        tower_index,
                        face,
                        bay,
                    },
                    frame: crate::WallLocalFrame {
                        origin,
                        tangent,
                        outward,
                        inside_room: None,
                        outside_room: None,
                    },
                    use_kind: crate::OpeningUse::BellOpening,
                    profile,
                    sill_elevation_metres: base + sill,
                    closure: crate::ClosurePolicy {
                        layers: vec![crate::ClosureKind::TimberLouvre],
                        state: crate::ClosureState::Open,
                        thickness_metres: 0.08,
                        swing_clearance_metres: 0.0,
                    },
                    head_kind: crate::OpeningHeadKind::PointedVoussoir,
                    void_id,
                    jamb_solids,
                    sill_solid: Some(sill_solid),
                    head_solid,
                    spandrel_solid,
                    reveal_surfaces,
                    closure_solids,
                    jamb_nodes,
                    head_node,
                    spandrel_node,
                    tracery_node: None,
                    stance_surface: None,
                    mount_solid: None,
                    ray_indices: Vec::new(),
                    sectional_void: (0..=8)
                        .map(|index| crate::OpeningVoidSlice {
                            depth_fraction: index as f32 / 8.0,
                            width_metres: width,
                            height_metres: apex,
                        })
                        .collect(),
                    head_bearing_interfaces,
                    wall_above_interface,
                });
                walls.push(crate::WallAssembly {
                    id: wall_id,
                    owner,
                    source: crate::WallSourceId::SquareTowerFace {
                        tower_index,
                        face,
                        bay,
                    },
                    material: crate::WallMaterialClass::CathedralMasonry,
                    storey_level: 2,
                    frame: crate::WallLocalFrame {
                        origin,
                        tangent,
                        outward,
                        inside_room: None,
                        outside_room: None,
                    },
                    radial_frame: None,
                    length_metres: bay_length,
                    height_metres: tower.wall_height_metres - 8.0,
                    base_elevation_metres: 8.0,
                    thickness_metres: thickness,
                    structural_role: crate::WallStructuralRole::LoadBearing,
                    support_node: wall_node,
                    host_solids,
                    opening_ids: vec![opening_id],
                    replaced_by_owner: None,
                });
            }
        }
    }
}

#[derive(Clone, Copy)]
struct ChurchWindowProfile {
    sill_metres: f32,
    width_metres: f32,
    spring_height_metres: f32,
    apex_height_metres: f32,
}

/// Replace one authoritative cathedral wall panel with a load-bearing,
/// two-light pointed opening.  This deliberately reuses the accepted Stage 3
/// wall/opening truth vocabulary: the source host is removed, the remaining
/// masonry is resolved around a full-depth void, and the stone mullion bears
/// on the sill rather than hanging from the arch.
fn resolve_church_pointed_window(
    wall: &mut crate::WallAssembly,
    opening_id: crate::OpeningAssemblyId,
    serial: u64,
    profile: ChurchWindowProfile,
    geometry: &mut ResolvedGeometry,
) -> crate::OpeningAssembly {
    let owner = wall.owner;
    let origin = wall.frame.origin;
    let tangent = wall.frame.tangent;
    let outward = wall.frame.outward;
    let thickness = wall.thickness_metres;
    let base = wall.base_elevation_metres;
    let wall_top = base + wall.height_metres;
    let sill = profile.sill_metres;
    let width = profile.width_metres;
    let spring = profile.spring_height_metres;
    let apex = profile.apex_height_metres;
    let radius = two_centred_arc_radius(width, apex - spring);
    let slot = 0x20_000 + serial * 0x40;

    let removed = wall.host_solids.clone();
    geometry.solids.retain(|solid| !removed.contains(&solid.id));
    let removed_interfaces = removed
        .iter()
        .map(|id| ResolvedItemId((4_u64 << 60) | (id.0 & 0x0FFF_FFFF_FFFF_FFFF)))
        .collect::<HashSet<_>>();
    geometry
        .support_interfaces
        .retain(|interface| !removed_interfaces.contains(&interface.id));

    let jamb_nodes = [
        StructuralNodeId(8_000_000 + serial * 8),
        StructuralNodeId(8_000_001 + serial * 8),
    ];
    for (side, node_id) in [-1.0_f32, 1.0].into_iter().zip(jamb_nodes) {
        geometry.structural_nodes.push(StructuralNode {
            id: node_id,
            owner,
            kind: StructuralNodeKind::OpeningJamb,
            position: Vec3::new(
                origin.x + tangent.x * side * width * 0.5,
                base,
                origin.y + tangent.y * side * width * 0.5,
            ),
            supported_by: vec![wall.support_node],
            grounded: false,
        });
    }
    let head_node = StructuralNodeId(8_000_002 + serial * 8);
    let spandrel_node = StructuralNodeId(8_000_003 + serial * 8);
    let tracery_node = StructuralNodeId(8_000_004 + serial * 8);
    geometry.structural_nodes.extend([
        StructuralNode {
            id: head_node,
            owner,
            kind: StructuralNodeKind::OpeningHead,
            position: Vec3::new(origin.x, sill + apex, origin.y),
            supported_by: jamb_nodes.to_vec(),
            grounded: false,
        },
        StructuralNode {
            id: spandrel_node,
            owner,
            kind: StructuralNodeKind::OpeningSpandrel,
            position: Vec3::new(origin.x, sill + apex + 0.20, origin.y),
            supported_by: vec![head_node],
            grounded: false,
        },
        StructuralNode {
            id: tracery_node,
            owner,
            kind: StructuralNodeKind::MullionBearing,
            position: Vec3::new(origin.x, sill, origin.y),
            supported_by: vec![wall.support_node],
            grounded: false,
        },
    ]);

    let side_width = (wall.length_metres - width) * 0.5;
    let mut jamb_solids = [ResolvedItemId::default(); 2];
    let mut host_solids = Vec::new();
    for (index, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
        let point = origin + tangent * side * (width * 0.5 + side_width * 0.5);
        let id = wall_solid(
            geometry,
            owner,
            slot + index as u64,
            Vec3::new(point.x, base + wall.height_metres * 0.5, point.y),
            if tangent.x.abs() > 0.5 {
                Vec3::new(side_width, wall.height_metres, thickness)
            } else {
                Vec3::new(thickness, wall.height_metres, side_width)
            },
            SolidRole::OpeningJamb,
            crate::ResolvedSolidShape::Cuboid,
            jamb_nodes[index],
        );
        jamb_solids[index] = id;
        host_solids.push(id);
    }
    let sill_height = sill - base;
    let sill_solid = wall_solid(
        geometry,
        owner,
        slot + 2,
        Vec3::new(origin.x, base + sill_height * 0.5, origin.y),
        if tangent.x.abs() > 0.5 {
            Vec3::new(width, sill_height, thickness)
        } else {
            Vec3::new(thickness, sill_height, width)
        },
        SolidRole::OpeningSill,
        crate::ResolvedSolidShape::Cuboid,
        wall.support_node,
    );
    host_solids.push(sill_solid);
    let ring_depth = 0.24_f32;
    let head_solid = wall_solid(
        geometry,
        owner,
        slot + 3,
        Vec3::new(
            origin.x,
            sill + (spring + apex + ring_depth) * 0.5,
            origin.y,
        ),
        if tangent.x.abs() > 0.5 {
            Vec3::new(width + 0.30, apex - spring + ring_depth, thickness)
        } else {
            Vec3::new(thickness, apex - spring + ring_depth, width + 0.30)
        },
        SolidRole::OpeningHead,
        crate::ResolvedSolidShape::PointedArchRing {
            clear_span_metres: width,
            spring_height_metres: spring,
            apex_height_metres: apex,
            arc_radius_metres: radius,
            ring_depth_metres: ring_depth,
        },
        head_node,
    );
    host_solids.push(head_solid);
    let spandrel_bottom = sill + apex + ring_depth - 0.025;
    let spandrel_height = (wall_top - spandrel_bottom).max(0.08);
    let spandrel_solid = wall_solid(
        geometry,
        owner,
        slot + 4,
        Vec3::new(origin.x, spandrel_bottom + spandrel_height * 0.5, origin.y),
        if tangent.x.abs() > 0.5 {
            Vec3::new(width + 0.30, spandrel_height, thickness)
        } else {
            Vec3::new(thickness, spandrel_height, width + 0.30)
        },
        SolidRole::OpeningSpandrel,
        crate::ResolvedSolidShape::Cuboid,
        spandrel_node,
    );
    host_solids.push(spandrel_solid);

    let mullion_height = spring;
    let mullion = wall_solid(
        geometry,
        owner,
        slot + 5,
        Vec3::new(
            origin.x,
            sill - 0.0125 + (mullion_height + 0.025) * 0.5,
            origin.y,
        ),
        if tangent.x.abs() > 0.5 {
            Vec3::new(0.10, mullion_height + 0.025, thickness * 0.36)
        } else {
            Vec3::new(thickness * 0.36, mullion_height + 0.025, 0.10)
        },
        SolidRole::Mullion,
        crate::ResolvedSolidShape::Cuboid,
        tracery_node,
    );
    let transom = wall_solid(
        geometry,
        owner,
        slot + 6,
        Vec3::new(origin.x, sill + spring * 0.70, origin.y),
        if tangent.x.abs() > 0.5 {
            Vec3::new(width * 0.82, 0.10, thickness * 0.32)
        } else {
            Vec3::new(thickness * 0.32, 0.10, width * 0.82)
        },
        SolidRole::Mullion,
        crate::ResolvedSolidShape::Cuboid,
        tracery_node,
    );
    host_solids.extend([mullion, transom]);

    let half_tangent = tangent.abs() * width * 0.5;
    let half_depth = outward.abs() * thickness * 0.55;
    let depth_sign = if tangent.x.abs() > 0.5 {
        if outward.y >= 0.0 { 1 } else { -1 }
    } else if outward.x <= 0.0 {
        1
    } else {
        -1
    };
    let void_id = wall_void(
        geometry,
        owner,
        slot,
        ResolvedBounds {
            min: Vec3::new(
                origin.x - half_tangent.x - half_depth.x,
                sill,
                origin.y - half_tangent.y - half_depth.y,
            ),
            max: Vec3::new(
                origin.x + half_tangent.x + half_depth.x,
                sill + apex,
                origin.y + half_tangent.y + half_depth.y,
            ),
        },
        opening_id,
        width,
        width,
        apex,
        apex,
        depth_sign,
    );
    let mut reveal_surfaces = Vec::new();
    for (surface_slot, side, role) in [
        (slot + 10, -1_i8, SurfaceRole::LeftJambReveal),
        (slot + 11, 1_i8, SurfaceRole::RightJambReveal),
    ] {
        let point = origin + tangent * f32::from(side) * width * 0.5;
        let extent = outward.abs() * thickness * 0.5 + tangent.abs() * 0.015;
        reveal_surfaces.push(wall_shaped_surface(
            geometry,
            owner,
            surface_slot,
            ResolvedBounds {
                min: Vec3::new(point.x - extent.x, sill, point.y - extent.y),
                max: Vec3::new(point.x + extent.x, sill + apex, point.y + extent.y),
            },
            role,
            crate::ResolvedSurfaceShape::SplayedJamb {
                side,
                exterior_width_metres: width,
                interior_width_metres: width,
                exterior_depth_sign: depth_sign,
            },
        ));
    }
    reveal_surfaces.push(wall_shaped_surface(
        geometry,
        owner,
        slot + 12,
        ResolvedBounds {
            min: Vec3::new(
                origin.x - half_tangent.x - half_depth.x,
                sill - 0.035,
                origin.y - half_tangent.y - half_depth.y,
            ),
            max: Vec3::new(
                origin.x + half_tangent.x + half_depth.x,
                sill + 0.015,
                origin.y + half_tangent.y + half_depth.y,
            ),
        },
        SurfaceRole::WeatherSill,
        crate::ResolvedSurfaceShape::WeatherSill {
            interior_elevation_metres: sill,
            exterior_elevation_metres: sill - 0.035,
            drip_depth_metres: 0.025,
        },
    ));
    reveal_surfaces.push(wall_shaped_surface(
        geometry,
        owner,
        slot + 13,
        ResolvedBounds {
            min: Vec3::new(
                origin.x - half_tangent.x - half_depth.x,
                sill + spring - 0.015,
                origin.y - half_tangent.y - half_depth.y,
            ),
            max: Vec3::new(
                origin.x + half_tangent.x + half_depth.x,
                sill + apex,
                origin.y + half_tangent.y + half_depth.y,
            ),
        },
        SurfaceRole::Intrados,
        crate::ResolvedSurfaceShape::PointedIntrados {
            clear_span_metres: width,
            spring_height_metres: spring,
            apex_height_metres: apex,
            arc_radius_metres: radius,
        },
    ));
    for (surface_slot, sign, role) in [
        (slot + 14, 1.0_f32, SurfaceRole::ExteriorThroat),
        (slot + 15, -1.0, SurfaceRole::InteriorMouth),
    ] {
        let point = origin + outward * thickness * 0.5 * sign;
        let depth = outward.abs() * 0.006;
        reveal_surfaces.push(wall_shaped_surface(
            geometry,
            owner,
            surface_slot,
            ResolvedBounds {
                min: Vec3::new(
                    point.x - half_tangent.x - depth.x,
                    sill,
                    point.y - half_tangent.y - depth.y,
                ),
                max: Vec3::new(
                    point.x + half_tangent.x + depth.x,
                    sill + apex,
                    point.y + half_tangent.y + depth.y,
                ),
            },
            role,
            crate::ResolvedSurfaceShape::Planar,
        ));
    }

    let panel_width = (width - 0.12) * 0.5;
    let panel_offset = panel_width * 0.5 + 0.03;
    let glazing_plan = origin - outward * thickness * 0.20;
    let mut closure_solids = Vec::new();
    for (index, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
        let point = glazing_plan + tangent * side * panel_offset;
        closure_solids.push(wall_solid(
            geometry,
            owner,
            slot + 20 + index as u64,
            Vec3::new(point.x, sill + apex * 0.5, point.y),
            if tangent.x.abs() > 0.5 {
                Vec3::new(panel_width * 0.94, apex, 0.025)
            } else {
                Vec3::new(0.025, apex, panel_width * 0.94)
            },
            SolidRole::LeadedGlazing,
            crate::ResolvedSolidShape::PointedArchRing {
                clear_span_metres: panel_width,
                spring_height_metres: spring,
                apex_height_metres: apex,
                arc_radius_metres: two_centred_arc_radius(panel_width, apex - spring),
                ring_depth_metres: 0.025,
            },
            tracery_node,
        ));
    }

    let bearing_width = 0.15_f32;
    let head_bearing_interfaces = [-1.0_f32, 1.0].map(|side| {
        let local = if side < 0.0 { slot + 50 } else { slot + 51 };
        let point = origin + tangent * side * (width * 0.5 + bearing_width * 0.5);
        let extent = tangent.abs() * bearing_width * 0.5 + outward.abs() * thickness * 0.5;
        let id = ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | local);
        geometry.support_interfaces.push(SupportInterface {
            id,
            owner,
            node: head_node,
            bounds: ResolvedBounds {
                min: Vec3::new(
                    point.x - extent.x,
                    sill + spring - 0.025,
                    point.y - extent.y,
                ),
                max: Vec3::new(
                    point.x + extent.x,
                    sill + spring + 0.025,
                    point.y + extent.y,
                ),
            },
        });
        id
    });
    let wall_above_interface =
        ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | (slot + 52));
    geometry.support_interfaces.push(SupportInterface {
        id: wall_above_interface,
        owner,
        node: spandrel_node,
        bounds: ResolvedBounds {
            min: Vec3::new(
                origin.x - half_tangent.x - half_depth.x,
                spandrel_bottom - 0.025,
                origin.y - half_tangent.y - half_depth.y,
            ),
            max: Vec3::new(
                origin.x + half_tangent.x + half_depth.x,
                spandrel_bottom + 0.025,
                origin.y + half_tangent.y + half_depth.y,
            ),
        },
    });
    geometry.support_interfaces.push(SupportInterface {
        id: ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | (slot + 53)),
        owner,
        node: tracery_node,
        bounds: ResolvedBounds {
            min: Vec3::new(
                origin.x - tangent.x.abs() * 0.05 - outward.x.abs() * thickness * 0.18,
                sill - 0.025,
                origin.y - tangent.y.abs() * 0.05 - outward.y.abs() * thickness * 0.18,
            ),
            max: Vec3::new(
                origin.x + tangent.x.abs() * 0.05 + outward.x.abs() * thickness * 0.18,
                sill + 0.01,
                origin.y + tangent.y.abs() * 0.05 + outward.y.abs() * thickness * 0.18,
            ),
        },
    });

    // `wall_solid` emits local X-length/Z-depth cuboids.  Cardinal walls need
    // no transform; apse chords rotate every resolved masonry, mullion, and
    // glazing member into the authoritative wall-local frame.
    let wall_yaw = -tangent.y.atan2(tangent.x);
    for id in host_solids.iter().chain(&closure_solids) {
        if let Some(item) = geometry.solids.iter_mut().find(|item| item.id == *id) {
            item.yaw_radians = wall_yaw;
        }
    }
    wall.host_solids = host_solids;
    wall.opening_ids = vec![opening_id];
    crate::OpeningAssembly {
        id: opening_id,
        owner,
        host_wall: wall.id,
        host_source: wall.source,
        frame: wall.frame,
        use_kind: crate::OpeningUse::Window,
        profile: crate::OpeningProfile::PointedTwoCentred {
            width_metres: width,
            spring_height_metres: spring,
            apex_height_metres: apex,
            arc_radius_metres: radius,
        },
        sill_elevation_metres: sill,
        closure: crate::ClosurePolicy {
            layers: vec![crate::ClosureKind::LeadedGlazing],
            state: crate::ClosureState::Closed,
            thickness_metres: 0.025,
            swing_clearance_metres: 0.0,
        },
        head_kind: crate::OpeningHeadKind::PointedVoussoir,
        void_id,
        jamb_solids,
        sill_solid: Some(sill_solid),
        head_solid,
        spandrel_solid,
        reveal_surfaces,
        closure_solids,
        jamb_nodes,
        head_node,
        spandrel_node,
        tracery_node: Some(tracery_node),
        stance_surface: None,
        mount_solid: None,
        ray_indices: Vec::new(),
        sectional_void: (0..=8)
            .map(|index| crate::OpeningVoidSlice {
                depth_fraction: index as f32 / 8.0,
                width_metres: width,
                height_metres: apex,
            })
            .collect(),
        head_bearing_interfaces,
        wall_above_interface,
    }
}

fn resolve_church_assembly(
    program: &BuildingProgram,
    walls: &mut Vec<crate::WallAssembly>,
    openings: &mut Vec<crate::OpeningAssembly>,
    stairs: &mut Vec<Stair>,
    geometry: &mut ResolvedGeometry,
) -> crate::ChurchAssembly {
    let church_program = program
        .church_program
        .expect("cathedral fixture has a church program");
    let owner = GeometryOwnerId(70_000);
    let datum = crate::ChurchDatum {
        floor_metres: 0.0,
        aisle_eave_metres: 7.0,
        clerestory_sill_metres: 9.10,
        nave_eave_metres: 11.5,
        vault_crown_metres: 10.85,
        bell_floor_metres: 17.3,
    };
    let tower_size = Vec2::splat(5.4);
    let tower_centre = Vec2::new(2.7, 10.5);
    let nave_west = 5.4_f32;
    let bay = f32::from(church_program.bay_length_cells) * CELL_SIZE_METRES;
    let nave_axes_metres = (0..church_program.nave_bays)
        .map(|index| nave_west + (f32::from(index) + 1.0) * bay)
        .collect::<Vec<_>>();
    let crossing_axis_metres = nave_west + f32::from(church_program.nave_bays) * bay + bay * 0.5;
    let crossing_west = crossing_axis_metres - bay * 0.5;
    let crossing_east = crossing_axis_metres + bay * 0.5;
    let choir_axes_metres = (0..church_program.choir_bays)
        .map(|index| crossing_east + (f32::from(index) + 0.5) * bay)
        .collect::<Vec<_>>();
    let choir_east = crossing_east + f32::from(church_program.choir_bays) * bay;

    let next_node = std::cell::Cell::new(7_000_000_u64);
    let next_slot = std::cell::Cell::new(0x7000_u64);
    let node = |kind: StructuralNodeKind,
                position: Vec3,
                supported_by: Vec<StructuralNodeId>,
                grounded: bool,
                geometry: &mut ResolvedGeometry| {
        let id = StructuralNodeId(next_node.get());
        next_node.set(next_node.get() + 1);
        geometry.structural_nodes.push(StructuralNode {
            id,
            owner,
            kind,
            position,
            supported_by,
            grounded,
        });
        id
    };
    let solid = |centre: Vec3,
                 size: Vec3,
                 role: SolidRole,
                 supports: Vec<StructuralNodeId>,
                 geometry: &mut ResolvedGeometry| {
        let slot = next_slot.get();
        next_slot.set(slot + 1);
        let id = ResolvedItemId((1_u64 << 60) | (u64::from(owner.0) << 32) | slot);
        geometry.solids.push(ResolvedSolid {
            id,
            owner,
            centre,
            size,
            yaw_radians: 0.0,
            crossfall_radians: 0.0,
            longfall_radians: 0.0,
            role,
            shape: crate::ResolvedSolidShape::Cuboid,
            supported_by: supports.clone(),
        });
        for support in supports {
            geometry.support_interfaces.push(SupportInterface {
                id: ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | next_slot.get()),
                owner,
                node: support,
                bounds: ResolvedBounds {
                    min: Vec3::new(
                        centre.x - size.x * 0.5,
                        centre.y - size.y * 0.5 - 0.015,
                        centre.z - size.z * 0.5,
                    ),
                    max: Vec3::new(
                        centre.x + size.x * 0.5,
                        centre.y - size.y * 0.5 + 0.015,
                        centre.z + size.z * 0.5,
                    ),
                },
            });
            next_slot.set(next_slot.get() + 1);
        }
        id
    };

    // Three floor strips remain separate so aisle/nave route widths are
    // explicit and testable.  Their shared boundaries have zero overlap.
    let floor_node = node(
        StructuralNodeKind::WallBearing,
        Vec3::new(21.0, 0.0, 10.5),
        Vec::new(),
        true,
        geometry,
    );
    let mut floor_solids = Vec::new();
    for (z, width) in [(6.0_f32, 2.10_f32), (10.5, 5.10), (15.0, 2.10)] {
        floor_solids.push(solid(
            Vec3::new((nave_west + choir_east) * 0.5, 0.10, z),
            Vec3::new(choir_east - nave_west - 0.90, 0.20, width),
            SolidRole::ChurchFloor,
            vec![floor_node],
            geometry,
        ));
    }
    floor_solids.push(solid(
        Vec3::new(crossing_axis_metres, 0.10, 10.5),
        Vec3::new(bay - 0.90, 0.20, 17.10),
        SolidRole::ChurchFloor,
        vec![floor_node],
        geometry,
    ));

    // The church envelope replaces the generic cell-wall vocabulary.  Each
    // bay-length host is authoritative for its masonry, later opening cuts,
    // buttress station, and roof bearing.
    let mut church_wall_serial = 0_u64;
    let mut exterior_segments = Vec::new();
    for bay_index in 0..church_program.nave_bays {
        let x = nave_west + (f32::from(bay_index) + 0.5) * bay;
        exterior_segments.push((
            crate::ChurchRange::Nave,
            Direction::South,
            bay_index,
            Vec2::new(x, 4.5),
            Vec2::X,
            Vec2::NEG_Y,
            bay,
        ));
        exterior_segments.push((
            crate::ChurchRange::Nave,
            Direction::North,
            bay_index,
            Vec2::new(x, 16.5),
            Vec2::X,
            Vec2::Y,
            bay,
        ));
    }
    for bay_index in 0..church_program.choir_bays {
        let x = crossing_east + (f32::from(bay_index) + 0.5) * bay;
        exterior_segments.push((
            crate::ChurchRange::Choir,
            Direction::South,
            bay_index,
            Vec2::new(x, 7.5),
            Vec2::X,
            Vec2::NEG_Y,
            bay,
        ));
        exterior_segments.push((
            crate::ChurchRange::Choir,
            Direction::North,
            bay_index,
            Vec2::new(x, 13.5),
            Vec2::X,
            Vec2::Y,
            bay,
        ));
    }
    exterior_segments.extend([
        (
            crate::ChurchRange::Transept,
            Direction::South,
            0,
            Vec2::new(crossing_axis_metres, 1.5),
            Vec2::X,
            Vec2::NEG_Y,
            bay,
        ),
        (
            crate::ChurchRange::Transept,
            Direction::North,
            0,
            Vec2::new(crossing_axis_metres, 19.5),
            Vec2::X,
            Vec2::Y,
            bay,
        ),
        (
            crate::ChurchRange::Transept,
            Direction::West,
            0,
            Vec2::new(crossing_west, 4.5),
            Vec2::Y,
            Vec2::NEG_X,
            6.0,
        ),
        (
            crate::ChurchRange::Transept,
            Direction::West,
            1,
            Vec2::new(crossing_west, 16.5),
            Vec2::Y,
            Vec2::NEG_X,
            6.0,
        ),
        (
            crate::ChurchRange::Transept,
            Direction::East,
            0,
            Vec2::new(crossing_east, 4.5),
            Vec2::Y,
            Vec2::X,
            6.0,
        ),
        (
            crate::ChurchRange::Transept,
            Direction::East,
            1,
            Vec2::new(crossing_east, 16.5),
            Vec2::Y,
            Vec2::X,
            6.0,
        ),
        (
            crate::ChurchRange::Nave,
            Direction::West,
            0,
            Vec2::new(nave_west, 6.0),
            Vec2::Y,
            Vec2::NEG_X,
            3.0,
        ),
        (
            crate::ChurchRange::Nave,
            Direction::West,
            1,
            Vec2::new(nave_west, 15.0),
            Vec2::Y,
            Vec2::NEG_X,
            3.0,
        ),
    ]);
    for (range, side, bay_index, origin, tangent, outward, length) in exterior_segments {
        let wall_height = if matches!(
            range,
            crate::ChurchRange::Transept | crate::ChurchRange::Choir
        ) {
            datum.nave_eave_metres
        } else {
            datum.aisle_eave_metres
        };
        let wall_owner = owner;
        let wall_node = StructuralNodeId(7_100_000 + church_wall_serial);
        geometry.structural_nodes.push(StructuralNode {
            id: wall_node,
            owner: wall_owner,
            kind: StructuralNodeKind::WallBearing,
            position: Vec3::new(origin.x, 0.0, origin.y),
            supported_by: Vec::new(),
            grounded: true,
        });
        let host = wall_solid(
            geometry,
            wall_owner,
            0x500 + church_wall_serial,
            Vec3::new(origin.x, wall_height * 0.5, origin.y),
            if tangent.x.abs() > 0.5 {
                Vec3::new(length, wall_height, 0.90)
            } else {
                Vec3::new(0.90, wall_height, length)
            },
            SolidRole::WallHost,
            crate::ResolvedSolidShape::Cuboid,
            wall_node,
        );
        walls.push(crate::WallAssembly {
            id: crate::WallAssemblyId(7_200_000 + church_wall_serial),
            owner: wall_owner,
            source: crate::WallSourceId::ChurchExterior {
                range,
                side,
                bay: bay_index,
            },
            material: crate::WallMaterialClass::CathedralMasonry,
            storey_level: 0,
            frame: crate::WallLocalFrame {
                origin,
                tangent,
                outward,
                inside_room: None,
                outside_room: None,
            },
            radial_frame: None,
            length_metres: length,
            height_metres: wall_height,
            base_elevation_metres: 0.0,
            thickness_metres: 0.90,
            structural_role: crate::WallStructuralRole::Buttressed,
            support_node: wall_node,
            host_solids: vec![host],
            opening_ids: Vec::new(),
            replaced_by_owner: None,
        });
        church_wall_serial += 1;
    }

    let (west_tower_wall, west_portal_opening) = resolve_church_tower_door_wall(
        Direction::West,
        crate::OpeningAssemblyId(7_400_000),
        crate::WallAssemblyId(7_400_000),
        GeometryOwnerId(71_000),
        tower_centre,
        geometry,
    );
    let west_portal = west_portal_opening.id;
    walls.push(west_tower_wall);
    openings.push(west_portal_opening);
    let (east_tower_wall, nave_passage_opening) = resolve_church_tower_door_wall(
        Direction::East,
        crate::OpeningAssemblyId(7_400_001),
        crate::WallAssemblyId(7_400_001),
        GeometryOwnerId(71_001),
        tower_centre,
        geometry,
    );
    let nave_passage = nave_passage_opening.id;
    walls.push(east_tower_wall);
    openings.push(nave_passage_opening);
    for (serial, face) in [Direction::South, Direction::North].into_iter().enumerate() {
        let wall_owner = GeometryOwnerId(71_002 + serial as u32);
        let outward = direction_vector(face);
        let origin = tower_centre + outward * 2.70;
        let support = StructuralNodeId(7_600_000 + serial as u64);
        geometry.structural_nodes.push(StructuralNode {
            id: support,
            owner: wall_owner,
            kind: StructuralNodeKind::WallBearing,
            position: Vec3::new(origin.x, 0.0, origin.y),
            supported_by: Vec::new(),
            grounded: true,
        });
        let host = wall_solid(
            geometry,
            wall_owner,
            0,
            Vec3::new(origin.x, 8.65, origin.y),
            Vec3::new(5.40, 17.30, 0.90),
            SolidRole::WallHost,
            crate::ResolvedSolidShape::Cuboid,
            support,
        );
        walls.push(crate::WallAssembly {
            id: crate::WallAssemblyId(7_400_002 + serial as u64),
            owner: wall_owner,
            source: crate::WallSourceId::ChurchTowerFace {
                face,
                stage: crate::ChurchTowerStage::Stair,
                bay: 0,
            },
            material: crate::WallMaterialClass::CathedralMasonry,
            storey_level: 0,
            frame: crate::WallLocalFrame {
                origin,
                tangent: Vec2::X,
                outward,
                inside_room: None,
                outside_room: None,
            },
            radial_frame: None,
            length_metres: 5.40,
            height_metres: 17.30,
            base_elevation_metres: 0.0,
            thickness_metres: 0.90,
            structural_role: crate::WallStructuralRole::LoadBearing,
            support_node: support,
            host_solids: vec![host],
            opening_ids: Vec::new(),
            replaced_by_owner: None,
        });
    }

    // Bay spans share their west/east supports.  The former implementation
    // manufactured one pier at the east axis and let the arcade springing
    // depend on that one node; this boundary pair gives the first bay the
    // same two-ended bearing contract as every subsequent bay.
    // Keep the west springing on the established clerestory/roof-abutment
    // datum.  Moving this support half a bay east made the first arcade look
    // regular in isolation but silently detached both aisle-roof wall
    // abutments from their authoritative masonry host.
    let clerestory_west = nave_axes_metres[0] - bay;
    // The arcade/vault bearing sits inside the westwork return rather than on
    // the tower's east wall centreline.  The clerestory weather enclosure
    // continues to the roof-abutment datum below, but the grounded pier and
    // thrust member clear the tower shell by a positive 0.15 m.
    let nave_bearing_west = clerestory_west + 0.60;
    let mut previous_pier_nodes = [StructuralNodeId(0); 2];
    let mut previous_buttress_nodes = [StructuralNodeId(0); 2];
    for (side_index, side_sign) in [-1.0_f32, 1.0].into_iter().enumerate() {
        let arcade_z = 10.5 + side_sign * 3.0;
        let pier_node = node(
            StructuralNodeKind::ChurchPier,
            Vec3::new(nave_bearing_west, 0.0, arcade_z),
            Vec::new(),
            true,
            geometry,
        );
        solid(
            Vec3::new(nave_bearing_west, 3.55, arcade_z),
            Vec3::new(0.72, 7.10, 0.72),
            SolidRole::ChurchPier,
            vec![pier_node],
            geometry,
        );
        previous_pier_nodes[side_index] = pier_node;
        let outer_z = 10.5 + side_sign * 7.0;
        let buttress_node = node(
            StructuralNodeKind::ChurchButtress,
            Vec3::new(nave_bearing_west, 0.0, outer_z),
            Vec::new(),
            true,
            geometry,
        );
        solid(
            Vec3::new(nave_bearing_west, 3.2, outer_z),
            Vec3::new(0.85, 6.4, 1.10),
            SolidRole::WallButtress,
            vec![buttress_node],
            geometry,
        );
        previous_buttress_nodes[side_index] = buttress_node;
    }

    let mut bay_assemblies = Vec::new();
    for (index, axis) in nave_axes_metres.iter().copied().enumerate() {
        let mut pier_nodes = [StructuralNodeId(0); 2];
        let mut pier_solids = [ResolvedItemId(0); 2];
        let mut arcade_solids = [ResolvedItemId(0); 2];
        let mut arcade_bearing_nodes = [[StructuralNodeId(0); 2]; 2];
        let mut arcade_bearing_interfaces = [[ResolvedItemId(0); 2]; 2];
        let mut buttress_nodes = [StructuralNodeId(0); 2];
        let mut buttress_solids = [ResolvedItemId(0); 2];
        let mut vault_solids = Vec::new();
        let mut vault_thrust_solids = Vec::new();
        let mut vault_load_surfaces = Vec::new();
        let mut vault_spring_nodes = Vec::new();
        let mut vault_bearing_interfaces = Vec::new();
        let previous_axis = if index == 0 {
            nave_bearing_west
        } else {
            nave_axes_metres[index - 1]
        };
        for (side_index, side_sign) in [-1.0_f32, 1.0].into_iter().enumerate() {
            let arcade_z = 10.5 + side_sign * 3.0;
            let pier_node = node(
                StructuralNodeKind::ChurchPier,
                Vec3::new(axis, 0.0, arcade_z),
                Vec::new(),
                true,
                geometry,
            );
            pier_nodes[side_index] = pier_node;
            pier_solids[side_index] = solid(
                Vec3::new(axis, 3.55, arcade_z),
                Vec3::new(0.72, 7.10, 0.72),
                SolidRole::ChurchPier,
                vec![pier_node],
                geometry,
            );
            let spring_node = node(
                StructuralNodeKind::ChurchArcadeSpringing,
                Vec3::new((previous_axis + axis) * 0.5, 4.85, arcade_z),
                vec![previous_pier_nodes[side_index], pier_node],
                false,
                geometry,
            );
            arcade_bearing_nodes[side_index] = [previous_pier_nodes[side_index], pier_node];
            arcade_solids[side_index] = solid(
                Vec3::new((previous_axis + axis) * 0.5, 6.0, arcade_z),
                Vec3::new(axis - previous_axis, 2.30, 0.55),
                SolidRole::ChurchArcade,
                vec![spring_node],
                geometry,
            );
            if let Some(arcade) = geometry
                .solids
                .iter_mut()
                .find(|solid| solid.id == arcade_solids[side_index])
            {
                let rise = 1.60;
                arcade.shape = crate::ResolvedSolidShape::PointedArchRing {
                    clear_span_metres: axis - previous_axis,
                    spring_height_metres: 4.85,
                    apex_height_metres: 4.85 + rise,
                    arc_radius_metres: two_centred_arc_radius(axis - previous_axis, rise),
                    ring_depth_metres: 0.55,
                };
            }
            for (end_index, end_x) in [previous_axis, axis].into_iter().enumerate() {
                let interface_id = ResolvedItemId(
                    (4_u64 << 60)
                        | (u64::from(owner.0) << 32)
                        | (800 + index as u64 * 20 + side_index as u64 * 4 + end_index as u64),
                );
                geometry.support_interfaces.push(SupportInterface {
                    id: interface_id,
                    owner,
                    node: spring_node,
                    bounds: ResolvedBounds {
                        min: Vec3::new(end_x - 0.30, 4.78, arcade_z - 0.27),
                        max: Vec3::new(end_x + 0.30, 5.12, arcade_z + 0.27),
                    },
                });
                arcade_bearing_interfaces[side_index][end_index] = interface_id;
            }
            let clerestory_owner = owner;
            let clerestory_wall_id =
                crate::WallAssemblyId(7_300_000 + index as u64 * 2 + side_index as u64);
            let clerestory_span_west = if index == 0 {
                clerestory_west
            } else {
                previous_axis
            };
            let clerestory_origin = Vec2::new((clerestory_span_west + axis) * 0.5, arcade_z);
            let clerestory_host = wall_solid(
                geometry,
                clerestory_owner,
                0,
                Vec3::new(clerestory_origin.x, 9.30, clerestory_origin.y),
                Vec3::new(axis - clerestory_span_west, 4.40, 0.75),
                SolidRole::WallHost,
                crate::ResolvedSolidShape::Cuboid,
                pier_node,
            );
            walls.push(crate::WallAssembly {
                id: clerestory_wall_id,
                owner: clerestory_owner,
                source: crate::WallSourceId::ChurchArcade {
                    side: if side_sign < 0.0 {
                        Direction::South
                    } else {
                        Direction::North
                    },
                    bay: index as u8,
                },
                material: crate::WallMaterialClass::CathedralMasonry,
                storey_level: 1,
                frame: crate::WallLocalFrame {
                    origin: clerestory_origin,
                    tangent: Vec2::X,
                    outward: if side_sign < 0.0 {
                        Vec2::NEG_Y
                    } else {
                        Vec2::Y
                    },
                    inside_room: None,
                    outside_room: None,
                },
                radial_frame: None,
                length_metres: axis - clerestory_span_west,
                height_metres: 4.40,
                base_elevation_metres: 7.10,
                thickness_metres: 0.75,
                structural_role: crate::WallStructuralRole::LoadBearing,
                support_node: pier_node,
                host_solids: vec![clerestory_host],
                opening_ids: Vec::new(),
                replaced_by_owner: None,
            });
            let outer_z = 10.5 + side_sign * 7.0;
            let buttress_node = node(
                StructuralNodeKind::ChurchButtress,
                Vec3::new(axis, 0.0, outer_z),
                Vec::new(),
                true,
                geometry,
            );
            buttress_nodes[side_index] = buttress_node;
            buttress_solids[side_index] = solid(
                Vec3::new(axis, 3.2, outer_z),
                Vec3::new(0.85, 6.4, 1.10),
                SolidRole::WallButtress,
                vec![buttress_node],
                geometry,
            );
        }
        let west = previous_axis;
        for (side_index, side_sign) in [-1.0_f32, 1.0].into_iter().enumerate() {
            let spring = node(
                StructuralNodeKind::ChurchVaultSpringing,
                Vec3::new((west + axis) * 0.5, 7.1, 10.5 + side_sign * 3.0),
                vec![
                    previous_pier_nodes[side_index],
                    pier_nodes[side_index],
                    previous_buttress_nodes[side_index],
                    buttress_nodes[side_index],
                ],
                false,
                geometry,
            );
            vault_spring_nodes.push(spring);
            let vault = solid(
                Vec3::new((west + axis) * 0.5, 9.05, 10.5 + side_sign * 1.5),
                Vec3::new(axis - west - 0.10, 0.22, 3.20),
                SolidRole::ChurchVaultShell,
                vec![spring],
                geometry,
            );
            if let Some(resolved) = geometry.solids.iter_mut().find(|item| item.id == vault) {
                resolved.crossfall_radians = side_sign * 0.50;
            }
            vault_solids.push(vault);
            for bearing_x in [west, axis] {
                vault_thrust_solids.push(solid(
                    Vec3::new(bearing_x, 7.05, 10.5 + side_sign * 5.0),
                    Vec3::new(0.46, 0.34, 4.0),
                    SolidRole::ChurchVaultThrust,
                    vec![spring],
                    geometry,
                ));
            }
            for (bearing_index, (bearing_x, bearing_z)) in [
                (west, 10.5 + side_sign * 3.0),
                (axis, 10.5 + side_sign * 3.0),
                (west, 10.5 + side_sign * 7.0),
                (axis, 10.5 + side_sign * 7.0),
            ]
            .into_iter()
            .enumerate()
            {
                let interface_id = ResolvedItemId(
                    (4_u64 << 60)
                        | (u64::from(owner.0) << 32)
                        | (900 + index as u64 * 20 + side_index as u64 * 6 + bearing_index as u64),
                );
                geometry.support_interfaces.push(SupportInterface {
                    id: interface_id,
                    owner,
                    node: spring,
                    bounds: ResolvedBounds {
                        min: Vec3::new(bearing_x - 0.22, 6.95, bearing_z - 0.22),
                        max: Vec3::new(bearing_x + 0.22, 7.22, bearing_z + 0.22),
                    },
                });
                vault_bearing_interfaces.push(interface_id);
            }
            let surface_id = wall_surface(
                geometry,
                owner,
                next_slot.get(),
                ResolvedBounds {
                    min: Vec3::new(west, 7.0, 7.5),
                    max: Vec3::new(axis, datum.vault_crown_metres, 13.5),
                },
                SurfaceRole::ChurchVaultLoad,
            );
            next_slot.set(next_slot.get() + 1);
            vault_load_surfaces.push(surface_id);
        }
        bay_assemblies.push(crate::ChurchBayAssembly {
            axis_index: index as u8,
            axis_metres: axis,
            range: crate::ChurchRange::Nave,
            pier_nodes,
            pier_solids,
            arcade_solids,
            arcade_bearing_nodes,
            arcade_bearing_interfaces,
            buttress_nodes,
            buttress_solids,
            clerestory_openings: [crate::OpeningAssemblyId::default(); 2],
            vault_solids,
            vault_thrust_solids,
            vault_load_surfaces,
            vault_spring_nodes,
            vault_bearing_interfaces,
        });
        previous_pier_nodes = pier_nodes;
        previous_buttress_nodes = buttress_nodes;
    }

    let crossing_positions = [
        Vec2::new(crossing_west, 7.5),
        Vec2::new(crossing_west, 13.5),
        Vec2::new(crossing_east, 7.5),
        Vec2::new(crossing_east, 13.5),
    ];
    let mut crossing_nodes = [StructuralNodeId(0); 4];
    let mut crossing_piers = [ResolvedItemId(0); 4];
    for (index, position) in crossing_positions.into_iter().enumerate() {
        let support = node(
            StructuralNodeKind::ChurchCrossingPier,
            Vec3::new(position.x, 0.0, position.y),
            Vec::new(),
            true,
            geometry,
        );
        crossing_nodes[index] = support;
        crossing_piers[index] = solid(
            Vec3::new(position.x, 5.1, position.y),
            Vec3::new(1.05, 10.2, 1.05),
            SolidRole::ChurchPier,
            vec![support],
            geometry,
        );
    }
    let mut crossing_arches = [ResolvedItemId(0); 4];
    let mut crossing_arch_bearing_nodes = [[StructuralNodeId(0); 2]; 4];
    let mut crossing_arch_bearing_interfaces = [[ResolvedItemId(0); 2]; 4];
    for (index, (centre, size, supports)) in [
        (
            Vec3::new(crossing_axis_metres, 9.1, 7.5),
            Vec3::new(bay, 1.0, 0.70),
            vec![crossing_nodes[0], crossing_nodes[2]],
        ),
        (
            Vec3::new(crossing_axis_metres, 9.1, 13.5),
            Vec3::new(bay, 1.0, 0.70),
            vec![crossing_nodes[1], crossing_nodes[3]],
        ),
        (
            Vec3::new(crossing_west, 9.1, 10.5),
            Vec3::new(0.70, 1.0, 6.0),
            vec![crossing_nodes[0], crossing_nodes[1]],
        ),
        (
            Vec3::new(crossing_east, 9.1, 10.5),
            Vec3::new(0.70, 1.0, 6.0),
            vec![crossing_nodes[2], crossing_nodes[3]],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let arch_spring = node(
            StructuralNodeKind::ChurchArcadeSpringing,
            Vec3::new(centre.x, 5.75, centre.z),
            supports.clone(),
            false,
            geometry,
        );
        let arch_height = 3.0;
        crossing_arches[index] = solid(
            Vec3::new(centre.x, 7.25, centre.z),
            Vec3::new(size.x, arch_height, size.z),
            SolidRole::ChurchCrossingArch,
            vec![arch_spring],
            geometry,
        );
        let span = size.x.max(size.z);
        if let Some(arch) = geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == crossing_arches[index])
        {
            let rise = 2.0;
            arch.shape = crate::ResolvedSolidShape::PointedArchRing {
                clear_span_metres: span,
                spring_height_metres: 5.75,
                apex_height_metres: 5.75 + rise,
                arc_radius_metres: two_centred_arc_radius(span, rise),
                ring_depth_metres: size.x.min(size.z),
            };
        }
        crossing_arch_bearing_nodes[index] = [supports[0], supports[1]];
        let along_x = size.x > size.z;
        for (end_index, sign) in [-1.0_f32, 1.0].into_iter().enumerate() {
            let contact = if along_x {
                Vec3::new(centre.x + sign * span * 0.5, 6.0, centre.z)
            } else {
                Vec3::new(centre.x, 6.0, centre.z + sign * span * 0.5)
            };
            let interface = ResolvedItemId(
                (4_u64 << 60)
                    | (u64::from(owner.0) << 32)
                    | (1_200 + index as u64 * 2 + end_index as u64),
            );
            geometry.support_interfaces.push(SupportInterface {
                id: interface,
                owner,
                node: arch_spring,
                bounds: ResolvedBounds {
                    min: contact - Vec3::new(0.28, 0.25, 0.28),
                    max: contact + Vec3::new(0.28, 0.25, 0.28),
                },
            });
            crossing_arch_bearing_interfaces[index][end_index] = interface;
        }
    }
    let mut crossing_buttress_nodes = [StructuralNodeId(0); 4];
    let mut crossing_buttress_solids = [ResolvedItemId(0); 4];
    let mut crossing_thrust_solids = Vec::new();
    let mut crossing_vault_bearings = Vec::new();
    for (index, pier_position) in crossing_positions.into_iter().enumerate() {
        let outward_z = if pier_position.y < 10.5 { -1.0 } else { 1.0 };
        let buttress_position = Vec2::new(pier_position.x, pier_position.y + outward_z * 2.0);
        let buttress = node(
            StructuralNodeKind::ChurchButtress,
            Vec3::new(buttress_position.x, 0.0, buttress_position.y),
            Vec::new(),
            true,
            geometry,
        );
        crossing_buttress_nodes[index] = buttress;
        crossing_buttress_solids[index] = solid(
            Vec3::new(buttress_position.x, 4.2, buttress_position.y),
            Vec3::new(1.05, 8.4, 1.25),
            SolidRole::WallButtress,
            vec![buttress],
            geometry,
        );
    }
    let crossing_vault_node = node(
        StructuralNodeKind::ChurchVaultSpringing,
        Vec3::new(crossing_axis_metres, 9.0, 10.5),
        crossing_nodes
            .iter()
            .chain(&crossing_buttress_nodes)
            .copied()
            .collect(),
        false,
        geometry,
    );
    for (index, pier_position) in crossing_positions.into_iter().enumerate() {
        let outward_z = if pier_position.y < 10.5 { -1.0 } else { 1.0 };
        crossing_thrust_solids.push(solid(
            Vec3::new(pier_position.x, 7.1, pier_position.y + outward_z),
            Vec3::new(0.48, 0.36, 2.0),
            SolidRole::ChurchVaultThrust,
            vec![crossing_vault_node],
            geometry,
        ));
        for (end, position) in [
            pier_position,
            Vec2::new(pier_position.x, pier_position.y + outward_z * 2.0),
        ]
        .into_iter()
        .enumerate()
        {
            let interface = ResolvedItemId(
                (4_u64 << 60)
                    | (u64::from(owner.0) << 32)
                    | (1_240 + index as u64 * 2 + end as u64),
            );
            geometry.support_interfaces.push(SupportInterface {
                id: interface,
                owner,
                node: crossing_vault_node,
                bounds: ResolvedBounds {
                    min: Vec3::new(position.x - 0.24, 6.92, position.y - 0.24),
                    max: Vec3::new(position.x + 0.24, 7.28, position.y + 0.24),
                },
            });
            crossing_vault_bearings.push(interface);
        }
    }
    let crossing_vaults = vec![solid(
        Vec3::new(crossing_axis_metres, 10.45, 10.5),
        Vec3::new(bay - 0.15, 0.24, 5.85),
        SolidRole::ChurchVaultShell,
        vec![crossing_vault_node],
        geometry,
    )];
    let crossing_load_surface = wall_surface(
        geometry,
        owner,
        next_slot.get(),
        ResolvedBounds {
            min: Vec3::new(crossing_west, 7.0, 7.5),
            max: Vec3::new(crossing_east, datum.vault_crown_metres, 13.5),
        },
        SurfaceRole::ChurchVaultLoad,
    );
    next_slot.set(next_slot.get() + 1);
    let crossing = crate::ChurchCrossingAssembly {
        bounds: ResolvedBounds {
            min: Vec3::new(crossing_west, 0.0, 7.5),
            max: Vec3::new(crossing_east, datum.vault_crown_metres, 13.5),
        },
        pier_nodes: crossing_nodes,
        pier_solids: crossing_piers,
        arch_solids: crossing_arches,
        arch_bearing_nodes: crossing_arch_bearing_nodes,
        arch_bearing_interfaces: crossing_arch_bearing_interfaces,
        vault_solids: crossing_vaults,
        buttress_nodes: crossing_buttress_nodes,
        buttress_solids: crossing_buttress_solids,
        vault_thrust_solids: crossing_thrust_solids,
        vault_load_surfaces: vec![crossing_load_surface],
        vault_spring_nodes: vec![crossing_vault_node],
        vault_bearing_interfaces: crossing_vault_bearings,
    };

    let mut choir_pier_nodes = Vec::new();
    let mut choir_pier_solids = Vec::new();
    let mut choir_buttress_nodes = Vec::new();
    let mut choir_buttress_solids = Vec::new();
    let mut choir_arch_solids = Vec::new();
    let mut choir_arch_bearing_nodes = Vec::new();
    let mut choir_arch_bearing_interfaces = Vec::new();
    let mut choir_vault_solids = Vec::new();
    let mut choir_vault_thrust_solids = Vec::new();
    let mut choir_vault_load_surfaces = Vec::new();
    let mut choir_vault_spring_nodes = Vec::new();
    let mut choir_vault_bearing_interfaces = Vec::new();
    let mut previous_choir_piers = [crossing_nodes[2], crossing_nodes[3]];
    let mut previous_choir_buttresses = [crossing_buttress_nodes[2], crossing_buttress_nodes[3]];
    for (bay_index, axis) in choir_axes_metres.iter().copied().enumerate() {
        let west = crossing_east + bay_index as f32 * bay;
        let east = west + bay;
        let mut current_piers = [StructuralNodeId(0); 2];
        let mut current_buttresses = [StructuralNodeId(0); 2];
        for (side_index, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
            let z = 10.5 + side * 3.0;
            let pier_node = node(
                StructuralNodeKind::ChurchPier,
                Vec3::new(east, 0.0, z),
                Vec::new(),
                true,
                geometry,
            );
            current_piers[side_index] = pier_node;
            choir_pier_nodes.push(pier_node);
            choir_pier_solids.push(solid(
                Vec3::new(east, 5.1, z),
                Vec3::new(0.78, 10.2, 0.78),
                SolidRole::ChurchPier,
                vec![pier_node],
                geometry,
            ));
            let buttress_node = node(
                StructuralNodeKind::ChurchButtress,
                Vec3::new(east, 0.0, 10.5 + side * 4.0),
                Vec::new(),
                true,
                geometry,
            );
            current_buttresses[side_index] = buttress_node;
            choir_buttress_nodes.push(buttress_node);
            choir_buttress_solids.push(solid(
                Vec3::new(east, 4.0, 10.5 + side * 4.0),
                Vec3::new(0.85, 8.0, 1.10),
                SolidRole::WallButtress,
                vec![buttress_node],
                geometry,
            ));
        }
        for (side_index, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
            let arcade_z = 10.5 + side * 3.0;
            let arch_spring = node(
                StructuralNodeKind::ChurchArcadeSpringing,
                Vec3::new((west + east) * 0.5, 6.6, arcade_z),
                vec![previous_choir_piers[side_index], current_piers[side_index]],
                false,
                geometry,
            );
            let arch = solid(
                Vec3::new((west + east) * 0.5, 6.2, arcade_z),
                Vec3::new(east - west, 2.60, 0.62),
                SolidRole::ChurchArcade,
                vec![arch_spring],
                geometry,
            );
            if let Some(item) = geometry.solids.iter_mut().find(|item| item.id == arch) {
                let rise = 1.75;
                item.shape = crate::ResolvedSolidShape::PointedArchRing {
                    clear_span_metres: east - west,
                    spring_height_metres: 4.90,
                    apex_height_metres: 4.90 + rise,
                    arc_radius_metres: two_centred_arc_radius(east - west, rise),
                    ring_depth_metres: 0.62,
                };
            }
            choir_arch_solids.push(arch);
            choir_arch_bearing_nodes
                .push([previous_choir_piers[side_index], current_piers[side_index]]);
            let mut arch_interfaces = [ResolvedItemId(0); 2];
            for (end_index, end_x) in [west, east].into_iter().enumerate() {
                let interface = ResolvedItemId(
                    (4_u64 << 60)
                        | (u64::from(owner.0) << 32)
                        | (1_300
                            + bay_index as u64 * 32
                            + side_index as u64 * 4
                            + end_index as u64),
                );
                geometry.support_interfaces.push(SupportInterface {
                    id: interface,
                    owner,
                    node: arch_spring,
                    bounds: ResolvedBounds {
                        min: Vec3::new(end_x - 0.28, 5.0, arcade_z - 0.28),
                        max: Vec3::new(end_x + 0.28, 5.4, arcade_z + 0.28),
                    },
                });
                arch_interfaces[end_index] = interface;
            }
            choir_arch_bearing_interfaces.push(arch_interfaces);
            let spring = node(
                StructuralNodeKind::ChurchVaultSpringing,
                Vec3::new(axis, 7.5, 10.5 + side * 3.0),
                vec![
                    previous_choir_piers[side_index],
                    current_piers[side_index],
                    previous_choir_buttresses[side_index],
                    current_buttresses[side_index],
                ],
                false,
                geometry,
            );
            choir_vault_spring_nodes.push(spring);
            let vault = solid(
                Vec3::new((west + east) * 0.5, 9.25, 10.5 + side * 1.5),
                Vec3::new(bay - 0.10, 0.22, 3.20),
                SolidRole::ChurchVaultShell,
                vec![spring],
                geometry,
            );
            if let Some(item) = geometry.solids.iter_mut().find(|item| item.id == vault) {
                item.crossfall_radians = side * 0.50;
            }
            choir_vault_solids.push(vault);
            for bearing_x in [west, east] {
                choir_vault_thrust_solids.push(solid(
                    Vec3::new(bearing_x, 7.35, 10.5 + side * 3.5),
                    Vec3::new(0.48, 0.34, 1.0),
                    SolidRole::ChurchVaultThrust,
                    vec![spring],
                    geometry,
                ));
            }
            for (bearing_index, (bearing_x, bearing_z)) in [
                (west, 10.5 + side * 3.0),
                (east, 10.5 + side * 3.0),
                (west, 10.5 + side * 4.0),
                (east, 10.5 + side * 4.0),
            ]
            .into_iter()
            .enumerate()
            {
                let interface = ResolvedItemId(
                    (4_u64 << 60)
                        | (u64::from(owner.0) << 32)
                        | (1_360
                            + bay_index as u64 * 32
                            + side_index as u64 * 8
                            + bearing_index as u64),
                );
                geometry.support_interfaces.push(SupportInterface {
                    id: interface,
                    owner,
                    node: spring,
                    bounds: ResolvedBounds {
                        min: Vec3::new(bearing_x - 0.23, 7.15, bearing_z - 0.23),
                        max: Vec3::new(bearing_x + 0.23, 7.52, bearing_z + 0.23),
                    },
                });
                choir_vault_bearing_interfaces.push(interface);
            }
        }
        let surface = wall_surface(
            geometry,
            owner,
            next_slot.get(),
            ResolvedBounds {
                min: Vec3::new(west, 7.0, 7.5),
                max: Vec3::new(east, datum.vault_crown_metres, 13.5),
            },
            SurfaceRole::ChurchVaultLoad,
        );
        next_slot.set(next_slot.get() + 1);
        choir_vault_load_surfaces.push(surface);
        previous_choir_piers = current_piers;
        previous_choir_buttresses = current_buttresses;
    }

    let mut apse_facets = Vec::new();
    let mut radial_nodes = Vec::new();
    let mut radial_solids = Vec::new();
    let apse_centre = Vec2::new(choir_east, 10.5);
    let radius = 4.45_f32;
    for facet in 0..5_u8 {
        let angle0 = -std::f32::consts::FRAC_PI_2 + f32::from(facet) * std::f32::consts::PI / 5.0;
        let angle1 =
            -std::f32::consts::FRAC_PI_2 + f32::from(facet + 1) * std::f32::consts::PI / 5.0;
        let start = apse_centre + Vec2::new(angle0.cos(), angle0.sin()) * radius;
        let end = apse_centre + Vec2::new(angle1.cos(), angle1.sin()) * radius;
        let origin = (start + end) * 0.5;
        let tangent = (end - start).normalize();
        let mut outward = Vec2::new(tangent.y, -tangent.x);
        if outward.dot(origin - apse_centre) < 0.0 {
            outward = -outward;
        }
        let facet_length = start.distance(end);
        let angle = tangent.y.atan2(tangent.x);
        let wall_owner = owner;
        let support = StructuralNodeId(next_node.get());
        next_node.set(next_node.get() + 1);
        geometry.structural_nodes.push(StructuralNode {
            id: support,
            owner: wall_owner,
            kind: StructuralNodeKind::WallBearing,
            position: Vec3::new(origin.x, 0.0, origin.y),
            supported_by: Vec::new(),
            grounded: true,
        });
        let id = crate::WallAssemblyId(7_100_000 + u64::from(facet));
        let host = wall_solid(
            geometry,
            wall_owner,
            0x600 + u64::from(facet),
            Vec3::new(origin.x, 5.675, origin.y),
            Vec3::new(facet_length, 11.35, 0.90),
            SolidRole::WallHost,
            crate::ResolvedSolidShape::Cuboid,
            support,
        );
        if let Some(item) = geometry.solids.iter_mut().find(|item| item.id == host) {
            item.yaw_radians = -angle;
        }
        apse_facets.push(id);
        let buttress_node = node(
            StructuralNodeKind::ChurchButtress,
            Vec3::new(origin.x, 0.0, origin.y),
            Vec::new(),
            true,
            geometry,
        );
        radial_nodes.push(buttress_node);
        let buttress = solid(
            Vec3::new(
                origin.x + outward.x * 1.075,
                3.6,
                origin.y + outward.y * 1.075,
            ),
            Vec3::new(0.72, 7.2, 1.25),
            SolidRole::WallButtress,
            vec![buttress_node],
            geometry,
        );
        if let Some(item) = geometry.solids.iter_mut().find(|item| item.id == buttress) {
            item.yaw_radians = outward.x.atan2(outward.y);
        }
        radial_solids.push(buttress);
        walls.push(crate::WallAssembly {
            id,
            owner: wall_owner,
            source: crate::WallSourceId::ChurchApse { facet },
            material: crate::WallMaterialClass::CathedralMasonry,
            storey_level: 0,
            frame: crate::WallLocalFrame {
                origin,
                tangent,
                outward,
                inside_room: None,
                outside_room: None,
            },
            radial_frame: None,
            length_metres: facet_length,
            height_metres: 11.35,
            base_elevation_metres: 0.0,
            thickness_metres: 0.90,
            structural_role: crate::WallStructuralRole::Buttressed,
            support_node: support,
            host_solids: vec![host],
            opening_ids: Vec::new(),
            replaced_by_owner: None,
        });
    }

    // One centered light per structural bay keeps the opening hierarchy tied
    // to the buttress/pier rhythm.  Transept end windows are deliberately
    // larger; the apse uses narrower radial lights.  Rich tracery is outside
    // the MVP, but every opening is already a real two-light stone assembly.
    let mut window_targets = Vec::new();
    for bay_index in 0..church_program.nave_bays {
        for side in [Direction::South, Direction::North] {
            window_targets.push((
                crate::WallSourceId::ChurchExterior {
                    range: crate::ChurchRange::Nave,
                    side,
                    bay: bay_index,
                },
                ChurchWindowProfile {
                    sill_metres: 1.70,
                    width_metres: 1.45,
                    spring_height_metres: 2.45,
                    apex_height_metres: 4.35,
                },
            ));
            window_targets.push((
                crate::WallSourceId::ChurchArcade {
                    side,
                    bay: bay_index,
                },
                ChurchWindowProfile {
                    // Clear the 8.635 m aisle-roof abutment and its upstand.
                    sill_metres: 9.10,
                    width_metres: 1.35,
                    spring_height_metres: 1.05,
                    apex_height_metres: 2.30,
                },
            ));
        }
    }
    for bay_index in 0..church_program.choir_bays {
        for side in [Direction::South, Direction::North] {
            window_targets.push((
                crate::WallSourceId::ChurchExterior {
                    range: crate::ChurchRange::Choir,
                    side,
                    bay: bay_index,
                },
                ChurchWindowProfile {
                    sill_metres: 2.15,
                    width_metres: 1.60,
                    spring_height_metres: 3.55,
                    apex_height_metres: 6.15,
                },
            ));
        }
    }
    for side in [Direction::South, Direction::North] {
        window_targets.push((
            crate::WallSourceId::ChurchExterior {
                range: crate::ChurchRange::Transept,
                side,
                bay: 0,
            },
            ChurchWindowProfile {
                sill_metres: 1.75,
                width_metres: 2.35,
                spring_height_metres: 4.35,
                apex_height_metres: 7.65,
            },
        ));
    }
    for facet in [0_u8, 1, 3, 4] {
        window_targets.push((
            crate::WallSourceId::ChurchApse { facet },
            ChurchWindowProfile {
                sill_metres: 2.20,
                width_metres: 1.30,
                spring_height_metres: 3.55,
                apex_height_metres: 5.95,
            },
        ));
    }
    for (serial, (source, profile)) in window_targets.into_iter().enumerate() {
        let wall = walls
            .iter_mut()
            .find(|wall| wall.source == source)
            .expect("church window host");
        let opening_id = crate::OpeningAssemblyId(7_500_000 + serial as u64);
        openings.push(resolve_church_pointed_window(
            wall,
            opening_id,
            serial as u64,
            profile,
            geometry,
        ));
    }
    for bay in &mut bay_assemblies {
        for (side_index, side) in [Direction::South, Direction::North].into_iter().enumerate() {
            bay.clerestory_openings[side_index] = openings
                .iter()
                .find(|opening| {
                    opening.host_source
                        == crate::WallSourceId::ChurchArcade {
                            side,
                            bay: bay.axis_index,
                        }
                })
                .expect("resolved clerestory light")
                .id;
        }
    }
    let choir = crate::ChurchChoirAssembly {
        bay_axes_metres: choir_axes_metres.clone(),
        pier_nodes: choir_pier_nodes,
        pier_solids: choir_pier_solids,
        buttress_nodes: choir_buttress_nodes,
        buttress_solids: choir_buttress_solids,
        arch_solids: choir_arch_solids,
        arch_bearing_nodes: choir_arch_bearing_nodes,
        arch_bearing_interfaces: choir_arch_bearing_interfaces,
        apse_facets,
        radial_buttress_nodes: radial_nodes,
        radial_buttress_solids: radial_solids,
        floor_solids: floor_solids.clone(),
        vault_solids: choir_vault_solids,
        vault_thrust_solids: choir_vault_thrust_solids,
        vault_load_surfaces: choir_vault_load_surfaces,
        vault_spring_nodes: choir_vault_spring_nodes,
        vault_bearing_interfaces: choir_vault_bearing_interfaces,
    };

    let mut tower_wall_supports = walls
        .iter()
        .filter(|wall| {
            matches!(
                wall.source,
                crate::WallSourceId::SquareTowerFace { .. }
                    | crate::WallSourceId::ChurchTowerFace { .. }
            )
        })
        .map(|wall| wall.support_node)
        .collect::<Vec<_>>();
    tower_wall_supports.sort_unstable_by_key(|id| id.0);
    tower_wall_supports.dedup();
    let bell_floor_node = node(
        StructuralNodeKind::ChurchTowerStage,
        Vec3::new(tower_centre.x, datum.bell_floor_metres, tower_centre.y),
        tower_wall_supports.clone(),
        false,
        geometry,
    );
    // The bell floor is a bearing ring, not a slab silently intersected by the
    // spiral.  A frozen 2.80 m square stairwell clears the 1.35 m outer tread
    // radius while the four surrounding slabs retain positive tower bearing.
    let outer = 4.25_f32;
    let stairwell = 2.45_f32;
    let ring = (outer - stairwell) * 0.5;
    let offset = (outer + stairwell) * 0.25;
    let mut bell_floor_solids = Vec::new();
    for (offset_x, offset_z, size_x, size_z) in [
        (0.0, -offset, outer, ring),
        (0.0, offset, outer, ring),
        (-offset, 0.0, ring, stairwell),
        (offset, 0.0, ring, stairwell),
    ] {
        bell_floor_solids.push(solid(
            Vec3::new(
                tower_centre.x + offset_x,
                datum.bell_floor_metres,
                tower_centre.y + offset_z,
            ),
            Vec3::new(size_x, 0.28, size_z),
            SolidRole::ChurchBellFloor,
            vec![bell_floor_node],
            geometry,
        ));
    }
    let frame_node = node(
        StructuralNodeKind::ChurchBellFrame,
        Vec3::new(
            tower_centre.x,
            datum.bell_floor_metres + 0.14,
            tower_centre.y,
        ),
        tower_wall_supports.clone(),
        false,
        geometry,
    );
    // Two wall-bearing cross beams are the accepted coarse bell frame.  The
    // earlier four-post cage consumed the only 0.90 m service ring; detailed
    // timber bracing remains an explicit visual refinement rather than a
    // false circulation obstacle.
    let bell_frame_solids = vec![
        solid(
            Vec3::new(
                tower_centre.x,
                datum.bell_floor_metres + 3.55,
                tower_centre.y,
            ),
            Vec3::new(4.50, 0.28, 0.30),
            SolidRole::ChurchBellFrame,
            vec![frame_node],
            geometry,
        ),
        solid(
            Vec3::new(
                tower_centre.x,
                datum.bell_floor_metres + 3.55,
                tower_centre.y,
            ),
            Vec3::new(0.30, 0.28, 4.50),
            SolidRole::ChurchBellFrame,
            vec![frame_node],
            geometry,
        ),
    ];
    let bell_solid = solid(
        Vec3::new(
            tower_centre.x,
            datum.bell_floor_metres + 2.85,
            tower_centre.y,
        ),
        Vec3::new(1.10, 1.00, 0.85),
        SolidRole::ChurchBell,
        vec![frame_node],
        geometry,
    );
    let stair_index = stairs.len();
    stairs.push(Stair::Spiral {
        centre: tower_centre,
        base_height_metres: 0.0,
        rise_metres: datum.bell_floor_metres,
        inner_radius_metres: 0.20,
        outer_radius_metres: 1.10,
        turns: 4.0,
        clockwise: true,
        tread_count: 72,
    });
    let stair_bearing_node = node(
        StructuralNodeKind::ChurchTowerStage,
        Vec3::new(tower_centre.x, 0.0, tower_centre.y),
        tower_wall_supports.clone(),
        false,
        geometry,
    );
    let stair_newel_solid = solid(
        Vec3::new(
            tower_centre.x,
            datum.bell_floor_metres * 0.5,
            tower_centre.y,
        ),
        Vec3::new(0.20, datum.bell_floor_metres + 0.5, 0.20),
        SolidRole::ChurchStairNewel,
        vec![stair_bearing_node],
        geometry,
    );
    let mut stair_tread_solids = Vec::new();
    let mut stair_tread_interfaces = Vec::new();
    for tread in 0..72_u16 {
        let progress = f32::from(tread) / 72.0;
        let angle = -progress * 4.0 * std::f32::consts::TAU;
        // The authoritative service line runs through the tread centre.  A
        // 0.95 m-wide tread centred between a 0.40 m newel and 1.35 m outer
        // radius left no physical 0.90 m occupant envelope at the newel.  The
        // A compact 0.20..1.10 m radial flight retains the full 0.90 m
        // project corridor while leaving a 0.90 m bearing ring inside the
        // authoritative tower shell.
        let radius = (0.20 + 1.10) * 0.5;
        let position = tower_centre + Vec2::new(angle.cos(), angle.sin()) * radius;
        let tread_id = solid(
            Vec3::new(position.x, progress * datum.bell_floor_metres, position.y),
            Vec3::new(0.90, 0.12, 0.34),
            SolidRole::ChurchStairTread,
            vec![stair_bearing_node],
            geometry,
        );
        if let Some(tread_solid) = geometry.solids.iter_mut().find(|item| item.id == tread_id) {
            tread_solid.yaw_radians = -angle;
        }
        let inner = tower_centre + Vec2::new(angle.cos(), angle.sin()) * 0.10;
        let interface_id =
            ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | (1_600 + u64::from(tread)));
        geometry.support_interfaces.push(SupportInterface {
            id: interface_id,
            owner,
            node: stair_bearing_node,
            bounds: ResolvedBounds {
                min: Vec3::new(
                    inner.x - 0.11,
                    progress * datum.bell_floor_metres - 0.07,
                    inner.y - 0.11,
                ),
                max: Vec3::new(
                    inner.x + 0.11,
                    progress * datum.bell_floor_metres + 0.07,
                    inner.y + 0.11,
                ),
            },
        });
        stair_tread_solids.push(tread_id);
        stair_tread_interfaces.push(interface_id);
    }
    let mut landing_solids = Vec::new();
    let mut guard_solids = Vec::new();
    for (level, height) in [5.8_f32, 11.6, datum.bell_floor_metres]
        .into_iter()
        .enumerate()
    {
        let landing_angle = -(height / datum.bell_floor_metres) * 4.0 * std::f32::consts::TAU;
        let radial = Vec2::new(landing_angle.cos(), landing_angle.sin());
        let landing_plan = tower_centre + radial * 1.30;
        landing_solids.push(solid(
            Vec3::new(landing_plan.x, height, landing_plan.y),
            Vec3::new(1.20, 0.18, 1.20),
            SolidRole::Landing,
            if (height - datum.bell_floor_metres).abs() <= 0.05 {
                vec![bell_floor_node]
            } else {
                tower_wall_supports.clone()
            },
            geometry,
        ));
        if (height - datum.bell_floor_metres).abs() > 0.05 {
            let guard_plan = tower_centre + radial * 2.05;
            let guard = solid(
                Vec3::new(guard_plan.x, height + 0.55, guard_plan.y),
                Vec3::new(0.10, 1.10, 1.20),
                SolidRole::ChurchGuard,
                tower_wall_supports.clone(),
                geometry,
            );
            if let Some(guard_solid) = geometry.solids.iter_mut().find(|solid| solid.id == guard) {
                guard_solid.yaw_radians = -landing_angle;
            }
            guard_solids.push(guard);
        }
        let _ = level;
    }
    // Three sides of the bell-floor stairwell are protected; the east side is
    // the positive-width arrival from the landing.
    for (dx, dz, sx, sz) in [
        // West guard stops at the 0.90 m ladder transfer opening.
        (-1.175_f32, -0.45_f32, 0.10_f32, 1.45_f32),
        (0.0, -1.175, 2.35, 0.10),
        (0.0, 1.175, 2.35, 0.10),
    ] {
        guard_solids.push(solid(
            Vec3::new(
                tower_centre.x + dx,
                datum.bell_floor_metres + 0.55,
                tower_centre.y + dz,
            ),
            Vec3::new(sx, 1.10, sz),
            SolidRole::ChurchGuard,
            vec![bell_floor_node],
            geometry,
        ));
    }
    // A compact fixed ladder supplies the roof stage without forcing the bell
    // floor stair through the bell envelope. It is deliberately coarse MVP
    // service architecture rather than ornamental joinery.
    let mut roof_ladder_solids = Vec::new();
    let ladder_x = tower_centre.x - 1.65;
    let ladder_z = tower_centre.y + 1.0;
    let ladder_bottom = datum.bell_floor_metres + 0.18;
    let ladder_top = 21.30;
    for dz in [-0.38_f32, 0.38] {
        roof_ladder_solids.push(solid(
            Vec3::new(ladder_x, (ladder_bottom + ladder_top) * 0.5, ladder_z + dz),
            Vec3::new(0.10, ladder_top - ladder_bottom, 0.10),
            SolidRole::ChurchServiceLadder,
            vec![bell_floor_node],
            geometry,
        ));
    }
    let rung_count = 13_u8;
    for rung in 0..rung_count {
        let t = f32::from(rung) / f32::from(rung_count - 1);
        roof_ladder_solids.push(solid(
            Vec3::new(
                ladder_x,
                ladder_bottom + (ladder_top - ladder_bottom) * t,
                ladder_z,
            ),
            Vec3::new(0.10, 0.08, 0.98),
            SolidRole::ChurchServiceLadder,
            vec![bell_floor_node],
            geometry,
        ));
    }
    let bell_openings = openings
        .iter()
        .filter(|opening| opening.use_kind == crate::OpeningUse::BellOpening)
        .map(|opening| opening.id)
        .collect::<Vec<_>>();
    let roof_service_surface = wall_surface(
        geometry,
        owner,
        next_slot.get(),
        ResolvedBounds {
            min: Vec3::new(tower_centre.x - 1.5, 21.3, tower_centre.y - 1.5),
            max: Vec3::new(tower_centre.x + 1.5, 21.32, tower_centre.y + 1.5),
        },
        SurfaceRole::ChurchServiceRoute,
    );

    // Ground-level circulation is resolved as four physical patches.  The
    // portal edges span from opposite sides of each 0.90 m wall and are later
    // sampled through every sectional-void slice by the audit.  The 1.80 m
    // route width intentionally matches the tower doors; it is a project
    // processional-width gate, not a universal church dimension.
    let exterior_approach_surface = wall_surface(
        geometry,
        owner,
        next_slot.get() + 1,
        ResolvedBounds {
            min: Vec3::new(tower_centre.x - 4.95, 0.20, tower_centre.y - 0.90),
            max: Vec3::new(tower_centre.x - 3.15, 0.22, tower_centre.y + 0.90),
        },
        SurfaceRole::ChurchPublicRoute,
    );
    let vestibule_surface = wall_surface(
        geometry,
        owner,
        next_slot.get() + 2,
        ResolvedBounds {
            // The authoritative shared node is the clear east side of the
            // vestibule, beside (not through) the spiral newel.  Public
            // procession crosses it on axis while BellService turns here.
            min: Vec3::new(tower_centre.x + 0.20, 0.20, tower_centre.y - 0.48),
            max: Vec3::new(tower_centre.x + 1.10, 0.22, tower_centre.y + 0.48),
        },
        SurfaceRole::ChurchPublicRoute,
    );
    let nave_entry_surface = wall_surface(
        geometry,
        owner,
        next_slot.get() + 3,
        ResolvedBounds {
            min: Vec3::new(tower_centre.x + 3.15, 0.20, tower_centre.y - 0.90),
            max: Vec3::new(tower_centre.x + 4.50, 0.22, tower_centre.y + 0.90),
        },
        SurfaceRole::ChurchPublicRoute,
    );
    let public_surface = wall_surface(
        geometry,
        owner,
        next_slot.get() + 4,
        ResolvedBounds {
            min: Vec3::new(tower_centre.x + 3.15, 0.20, tower_centre.y - 0.90),
            max: Vec3::new(choir_east + radius - 0.4, 0.22, tower_centre.y + 0.90),
        },
        SurfaceRole::ChurchPublicRoute,
    );
    let ring_offset = 1.675_f32;
    let bell_floor_corner_surfaces = [
        (ring_offset, -ring_offset),
        (-ring_offset, -ring_offset),
        (ring_offset, ring_offset),
        (-ring_offset, ring_offset),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (dx, dz))| {
        wall_surface(
            geometry,
            owner,
            next_slot.get() + 10 + index as u64,
            ResolvedBounds {
                min: Vec3::new(
                    tower_centre.x + dx - 0.45,
                    datum.bell_floor_metres + 0.14,
                    tower_centre.y + dz - 0.45,
                ),
                max: Vec3::new(
                    tower_centre.x + dx + 0.45,
                    datum.bell_floor_metres + 0.16,
                    tower_centre.y + dz + 0.45,
                ),
            },
            SurfaceRole::ChurchServiceRoute,
        )
    })
    .collect::<Vec<_>>();
    let route_edge = |from, to, through_opening| crate::ChurchRouteEdge {
        from,
        to,
        clear_width_metres: 0.95,
        clear_headroom_metres: 2.0,
        through_opening,
    };
    let mut bell_route_edges = vec![route_edge(vestibule_surface, stair_tread_solids[0], None)];
    for pair in stair_tread_solids.windows(2) {
        bell_route_edges.push(route_edge(pair[0], pair[1], None));
    }
    for (landing_index, height) in [5.8_f32, 11.6].into_iter().enumerate() {
        let tread_index = ((height / datum.bell_floor_metres) * 72.0)
            .round()
            .clamp(1.0, 70.0) as usize;
        bell_route_edges.push(route_edge(
            stair_tread_solids[tread_index],
            landing_solids[landing_index],
            None,
        ));
        bell_route_edges.push(route_edge(
            landing_solids[landing_index],
            stair_tread_solids[tread_index + 1],
            None,
        ));
    }
    bell_route_edges.push(route_edge(
        *stair_tread_solids.last().expect("church stair tread"),
        landing_solids[2],
        None,
    ));
    // Ring indices are south, north, west, east. Corner surfaces keep both
    // protected ways around the stairwell on the physical bearing ring rather
    // than allowing one graph edge to cut diagonally through its void.
    bell_route_edges.push(route_edge(landing_solids[2], bell_floor_solids[3], None));
    bell_route_edges.push(route_edge(
        bell_floor_solids[3],
        bell_floor_corner_surfaces[0],
        None,
    ));
    bell_route_edges.push(route_edge(
        bell_floor_corner_surfaces[0],
        bell_floor_solids[0],
        None,
    ));
    bell_route_edges.push(route_edge(
        bell_floor_solids[0],
        bell_floor_corner_surfaces[1],
        None,
    ));
    bell_route_edges.push(route_edge(
        bell_floor_corner_surfaces[1],
        bell_floor_solids[2],
        None,
    ));
    bell_route_edges.push(route_edge(
        bell_floor_solids[3],
        bell_floor_corner_surfaces[2],
        None,
    ));
    bell_route_edges.push(route_edge(
        bell_floor_corner_surfaces[2],
        bell_floor_solids[1],
        None,
    ));
    bell_route_edges.push(route_edge(
        bell_floor_solids[1],
        bell_floor_corner_surfaces[3],
        None,
    ));
    bell_route_edges.push(route_edge(
        bell_floor_corner_surfaces[3],
        bell_floor_solids[2],
        None,
    ));
    let ladder_rungs = roof_ladder_solids
        .iter()
        .copied()
        .skip(2)
        .collect::<Vec<_>>();
    bell_route_edges.push(route_edge(bell_floor_solids[2], ladder_rungs[0], None));
    for pair in ladder_rungs.windows(2) {
        bell_route_edges.push(route_edge(pair[0], pair[1], None));
    }
    bell_route_edges.push(route_edge(
        *ladder_rungs.last().expect("church roof ladder rung"),
        roof_service_surface,
        None,
    ));
    let mut bell_route_solids = stair_tread_solids.clone();
    bell_route_solids.extend(landing_solids.iter().copied());
    bell_route_solids.extend(bell_floor_solids.iter().copied());
    bell_route_solids.extend(ladder_rungs.iter().copied());
    let wall_ids = walls
        .iter()
        .filter(|wall| {
            matches!(
                wall.source,
                crate::WallSourceId::SquareTowerFace { .. }
                    | crate::WallSourceId::ChurchTowerFace { .. }
            )
        })
        .map(|wall| wall.id)
        .collect();
    let tower = crate::ChurchTowerAssembly {
        centre: tower_centre,
        footprint_size_metres: tower_size,
        wall_ids,
        west_portal,
        nave_passage,
        exterior_approach_surface,
        vestibule_surface,
        nave_entry_surface,
        stair_index,
        stair_bearing_node,
        stair_newel_solid,
        stair_tread_solids: stair_tread_solids.clone(),
        stair_tread_interfaces,
        landing_solids,
        guard_solids,
        bell_floor_solids,
        bell_floor_corner_surfaces: bell_floor_corner_surfaces.clone(),
        bell_frame_solids,
        bell_solid,
        bell_openings,
        roof_ladder_solids,
        roof_service_surface,
    };
    crate::ChurchAssembly {
        id: crate::ChurchAssemblyId(1),
        program: church_program,
        datum,
        west_elevation_metres: 0.0,
        nave_axes_metres,
        crossing_axis_metres,
        choir_axes_metres,
        bay_assemblies,
        crossing,
        choir,
        tower,
        circulation: vec![
            crate::ChurchCirculationRoute {
                kind: crate::ChurchRouteKind::PublicProcessional,
                waypoints: vec![
                    Vec3::new(tower_centre.x - 4.05, 0.20, tower_centre.y),
                    Vec3::new(tower_centre.x, 0.20, tower_centre.y),
                    Vec3::new(tower_centre.x + 3.825, 0.20, tower_centre.y),
                    Vec3::new(choir_east + radius - 0.4, 0.20, 10.5),
                ],
                width_metres: 1.80,
                headroom_metres: 2.95,
                surface_ids: vec![
                    exterior_approach_surface,
                    vestibule_surface,
                    nave_entry_surface,
                    public_surface,
                ],
                traversable_solid_ids: Vec::new(),
                edges: vec![
                    crate::ChurchRouteEdge {
                        from: exterior_approach_surface,
                        to: vestibule_surface,
                        clear_width_metres: 1.80,
                        clear_headroom_metres: 2.95,
                        through_opening: Some(west_portal),
                    },
                    crate::ChurchRouteEdge {
                        from: vestibule_surface,
                        to: nave_entry_surface,
                        clear_width_metres: 1.80,
                        clear_headroom_metres: 2.95,
                        through_opening: Some(nave_passage),
                    },
                    crate::ChurchRouteEdge {
                        from: nave_entry_surface,
                        to: public_surface,
                        clear_width_metres: 1.80,
                        clear_headroom_metres: 2.95,
                        through_opening: None,
                    },
                ],
                opening_ids: vec![west_portal, nave_passage],
            },
            crate::ChurchCirculationRoute {
                kind: crate::ChurchRouteKind::BellService,
                waypoints: vec![
                    Vec3::new(tower_centre.x, 0.20, tower_centre.y),
                    Vec3::new(tower_centre.x, datum.bell_floor_metres, tower_centre.y),
                ],
                width_metres: 0.95,
                headroom_metres: 2.0,
                surface_ids: std::iter::once(vestibule_surface)
                    .chain(bell_floor_corner_surfaces.iter().copied())
                    .chain(std::iter::once(roof_service_surface))
                    .collect(),
                traversable_solid_ids: bell_route_solids,
                edges: bell_route_edges,
                opening_ids: Vec::new(),
            },
        ],
        floor_solids,
        roof_assemblies: Vec::new(),
    }
}

/// Resolve the nave clerestory as the physical host of the aisle shed high
/// edges.  The former roof-only model labelled these edges as wall abutments
/// and then discarded them because no masonry actually occupied the contact
/// contour.  These two wall assemblies continue the arcade lines above the
/// aisle roofs; the roof resolver can therefore measure flashing contact
/// against real masonry rather than a synthetic proof surface.
#[allow(dead_code)]
fn resolve_cathedral_clerestory_walls(
    roofs: &[RoofPiece],
    walls: &mut Vec<crate::WallAssembly>,
    geometry: &mut ResolvedGeometry,
) {
    let lower_supports = |base: f32, walls: &[crate::WallAssembly]| {
        let mut supports = walls
            .iter()
            .filter(|wall| {
                wall.replaced_by_owner.is_none()
                    && (wall.base_elevation_metres + wall.height_metres - base).abs() <= 0.08
            })
            .map(|wall| wall.support_node)
            .collect::<Vec<_>>();
        supports.sort_unstable();
        supports.dedup();
        supports
    };

    for (slot, (roof_index, high_side, outward)) in [
        (1_usize, Direction::East, Vec2::NEG_X),
        (2_usize, Direction::West, Vec2::X),
    ]
    .into_iter()
    .enumerate()
    {
        let Some(shed) = roofs.get(roof_index).copied() else {
            continue;
        };
        let Some(polygon) = roof_face_polygons(shed, Some(high_side)).into_iter().next() else {
            continue;
        };
        let high = polygon
            .iter()
            .map(|point| point.y)
            .fold(f32::NEG_INFINITY, f32::max);
        let high_vertices = polygon
            .iter()
            .filter(|point| (point.y - high).abs() <= 0.01)
            .copied()
            .collect::<Vec<_>>();
        if high_vertices.len() != 2 {
            continue;
        }
        let contact_x = (high_vertices[0].x + high_vertices[1].x) * 0.5;
        let min_z = high_vertices
            .iter()
            .map(|point| point.z)
            .fold(f32::INFINITY, f32::min);
        let max_z = high_vertices
            .iter()
            .map(|point| point.z)
            .fold(f32::NEG_INFINITY, f32::max);
        let base = shed.base_height_metres;
        // The 0.24 m upstand above the contact is a project weathering gate,
        // not a historical universal dimension.
        let top = high + 0.24;
        let height = top - base;
        let length = max_z - min_z;
        // The shed terminates at the exterior face of the clerestory, not at
        // its centreline.  Keeping the masonry on the nave side avoids both a
        // buried roof edge and an additive flashing screen.
        let origin = Vec2::new(contact_x - outward.x * 0.90 * 0.5, (min_z + max_z) * 0.5);
        let owner = GeometryOwnerId(53_000 + slot as u32);
        let wall_id = crate::WallAssemblyId(900_000 + slot as u64);
        let node = StructuralNodeId(2_900_000 + slot as u64);
        let supports = lower_supports(base, walls);
        geometry.structural_nodes.push(StructuralNode {
            id: node,
            owner,
            kind: StructuralNodeKind::WallBearing,
            position: Vec3::new(origin.x, base, origin.y),
            supported_by: supports,
            grounded: false,
        });
        let host = wall_solid(
            geometry,
            owner,
            0xC100 + slot as u64,
            Vec3::new(origin.x, base + height * 0.5, origin.y),
            Vec3::new(length, height, 0.90),
            SolidRole::WallHost,
            crate::ResolvedSolidShape::Cuboid,
            node,
        );
        geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == host)
            .expect("new clerestory wall solid")
            .yaw_radians = std::f32::consts::FRAC_PI_2;
        walls.push(crate::WallAssembly {
            id: wall_id,
            owner,
            source: crate::WallSourceId::CathedralClerestory { side: high_side },
            material: crate::WallMaterialClass::CathedralMasonry,
            storey_level: 1,
            frame: crate::WallLocalFrame {
                origin,
                tangent: Vec2::Y,
                outward,
                inside_room: None,
                outside_room: None,
            },
            radial_frame: None,
            length_metres: length,
            height_metres: height,
            base_elevation_metres: base,
            thickness_metres: 0.90,
            structural_role: crate::WallStructuralRole::LoadBearing,
            support_node: node,
            host_solids: vec![host],
            opening_ids: Vec::new(),
            replaced_by_owner: None,
        });
    }
}

fn resolve_roof_child_front_openings(
    program: &BuildingProgram,
    dormers: &[RoofDormer],
    roofs: &mut [RoofAssembly],
    walls: &mut Vec<crate::WallAssembly>,
    openings: &mut Vec<crate::OpeningAssembly>,
    geometry: &mut ResolvedGeometry,
) {
    for (index, dormer) in dormers.iter().copied().enumerate() {
        let roof_id = RoofAssemblyId(1_000 + index as u64);
        let parent_owner = roofs
            .iter()
            .find(|roof| roof.id == roof_id)
            .and_then(|roof| roof.parent)
            .and_then(|parent| roofs.iter().find(|roof| roof.id == parent))
            .map(|roof| roof.owner);
        let parent_support_nodes = roofs
            .iter()
            .find(|roof| roof.id == roof_id)
            .and_then(|roof| roof.parent)
            .and_then(|parent| roofs.iter().find(|roof| roof.id == parent))
            .map(|roof| roof.support_nodes.clone())
            .unwrap_or_default();
        let Some(child) = roofs.iter_mut().find(|roof| roof.id == roof_id) else {
            continue;
        };
        let front_enclosure_id = ResolvedItemId((0xA_u64 << 60) | (roof_id.0 << 16) | 0x4100);
        let Some(front) = child
            .enclosure_faces
            .iter()
            .find(|face| face.id == front_enclosure_id)
            .cloned()
        else {
            continue;
        };
        child
            .enclosure_faces
            .retain(|face| face.id != front_enclosure_id);

        let wall_id = crate::WallAssemblyId(1_000_000 + index as u64);
        let opening_id = crate::OpeningAssemblyId(1_000_000 + index as u64);
        let owner = GeometryOwnerId(70_000 + index as u32);
        let outward = direction_vector(dormer.facing);
        let tangent = if outward.y.abs() > 0.5 {
            Vec2::X
        } else {
            Vec2::Y
        };
        let origin = dormer.centre;
        let width = front
            .polygon
            .iter()
            .map(|point| Vec2::new(point.x, point.z).dot(tangent))
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), value| {
                (min.min(value), max.max(value))
            });
        let width = width.1 - width.0;
        let facade_wall = (dormer.kind == DormerKind::TransverseGable)
            .then(|| {
                walls
                    .iter()
                    .filter(|wall| {
                        matches!(wall.source, crate::WallSourceId::StoreyWall { .. })
                            && wall.frame.outside_room.is_none()
                            && wall.frame.outward.dot(outward) > 0.99
                    })
                    .min_by(|left, right| {
                        left.frame
                            .origin
                            .distance(origin)
                            .total_cmp(&right.frame.origin.distance(origin))
                    })
                    .map(|wall| (wall.id, wall.support_node))
            })
            .flatten();
        let base = front
            .polygon
            .iter()
            .map(|point| point.y)
            .fold(f32::INFINITY, f32::min);
        let top = front
            .polygon
            .iter()
            .map(|point| point.y)
            .fold(f32::NEG_INFINITY, f32::max);
        let height = (top - base).max(1.15);
        let thickness = 0.20;
        let wall_node = StructuralNodeId((u64::from(owner.0) << 16) | 1);
        geometry.structural_nodes.push(StructuralNode {
            id: wall_node,
            owner,
            kind: StructuralNodeKind::RoofWallPlate,
            position: Vec3::new(origin.x, base, origin.y),
            supported_by: facade_wall
                .map(|(_, node)| vec![node])
                .unwrap_or_else(|| parent_support_nodes.clone()),
            grounded: false,
        });
        // The child facade/cheeks carry the child roof; the parent roof carries
        // their curb/trimmers.  Do not reverse this edge (wall -> child roof),
        // which forms a semantic cycle and previously encouraged generic
        // ground-to-eave fallback posts.
        for roof_node_id in &child.support_nodes {
            if let Some(roof_node) = geometry
                .structural_nodes
                .iter_mut()
                .find(|node| node.id == *roof_node_id)
            {
                roof_node.supported_by.push(wall_node);
                roof_node.supported_by.sort_unstable();
                roof_node.supported_by.dedup();
            }
        }
        let jamb_nodes = [
            StructuralNodeId(wall_node.0 + 1),
            StructuralNodeId(wall_node.0 + 2),
        ];
        for (side, node) in [-1.0_f32, 1.0].into_iter().zip(jamb_nodes) {
            geometry.structural_nodes.push(StructuralNode {
                id: node,
                owner,
                kind: StructuralNodeKind::OpeningJamb,
                position: Vec3::new(
                    origin.x + tangent.x * side * width * 0.31,
                    base,
                    origin.y + tangent.y * side * width * 0.31,
                ),
                supported_by: vec![wall_node],
                grounded: false,
            });
        }
        let head_node = StructuralNodeId(wall_node.0 + 3);
        let spandrel_node = StructuralNodeId(wall_node.0 + 4);
        geometry.structural_nodes.push(StructuralNode {
            id: head_node,
            owner,
            kind: StructuralNodeKind::OpeningHead,
            position: Vec3::new(origin.x, base + height * 0.78, origin.y),
            supported_by: jamb_nodes.to_vec(),
            grounded: false,
        });
        geometry.structural_nodes.push(StructuralNode {
            id: spandrel_node,
            owner,
            kind: StructuralNodeKind::OpeningSpandrel,
            position: Vec3::new(origin.x, top, origin.y),
            supported_by: vec![head_node],
            grounded: false,
        });

        let opening_width = (width - 0.42).clamp(0.48, 0.82);
        let sill_height = 0.22;
        let sill_elevation = base + sill_height;
        let clear_height = (height - sill_height - 0.22).clamp(0.68, 1.12);
        let head_height = 0.14;
        let head_bottom = sill_elevation + clear_height;
        let side_width = (width - opening_width) * 0.5;
        let local_size = |tangent_width: f32, vertical: f32, depth: f32| {
            if tangent.x.abs() > 0.5 {
                Vec3::new(tangent_width, vertical, depth)
            } else {
                Vec3::new(depth, vertical, tangent_width)
            }
        };
        let mut host_solids = Vec::new();
        let mut jamb_solids = [ResolvedItemId::default(); 2];
        for (slot, side, node, target) in [
            (0_u64, -1.0_f32, jamb_nodes[0], 0_usize),
            (1, 1.0, jamb_nodes[1], 1),
        ] {
            let plan = origin + tangent * side * (opening_width + side_width) * 0.5;
            let solid = wall_solid(
                geometry,
                owner,
                slot,
                Vec3::new(plan.x, base + height * 0.5, plan.y),
                local_size(side_width, height, thickness),
                SolidRole::OpeningJamb,
                crate::ResolvedSolidShape::Cuboid,
                node,
            );
            host_solids.push(solid);
            jamb_solids[target] = solid;
        }
        let sill_solid = wall_solid(
            geometry,
            owner,
            2,
            Vec3::new(origin.x, base + sill_height * 0.5, origin.y),
            local_size(opening_width, sill_height, thickness),
            SolidRole::OpeningSill,
            crate::ResolvedSolidShape::Cuboid,
            wall_node,
        );
        host_solids.push(sill_solid);
        let head_solid = wall_solid(
            geometry,
            owner,
            3,
            Vec3::new(origin.x, head_bottom + head_height * 0.5, origin.y),
            local_size(opening_width + 0.12, head_height, thickness),
            SolidRole::OpeningHead,
            crate::ResolvedSolidShape::Cuboid,
            head_node,
        );
        host_solids.push(head_solid);
        let spandrel_height = (top - (head_bottom + head_height) + 0.025).max(0.08);
        let spandrel_solid = wall_solid(
            geometry,
            owner,
            4,
            Vec3::new(origin.x, top - spandrel_height * 0.5, origin.y),
            local_size(opening_width, spandrel_height, thickness),
            SolidRole::OpeningSpandrel,
            crate::ResolvedSolidShape::Cuboid,
            spandrel_node,
        );
        host_solids.push(spandrel_solid);
        // Non-Fachwerk fixtures retain the compact Stage-3 child-front frame.
        // The five accepted civilian programs instead receive their opening-
        // first members from `TimberFrameAssembly`, so duplicating this legacy
        // four-piece overlay would create two competing structural authorities.
        for (slot, plan, centre_y, frame_size) in (timber_program_kind(program.archetype).is_none())
            .then_some([
                (
                    100_u64,
                    origin - tangent * (width * 0.5 - 0.055),
                    base + height * 0.5,
                    local_size(0.11, height, 0.08),
                ),
                (
                    101,
                    origin + tangent * (width * 0.5 - 0.055),
                    base + height * 0.5,
                    local_size(0.11, height, 0.08),
                ),
                (102, origin, base + 0.055, local_size(width, 0.11, 0.08)),
                (103, origin, top - 0.055, local_size(width, 0.11, 0.08)),
            ])
            .into_iter()
            .flatten()
        {
            host_solids.push(wall_solid(
                geometry,
                owner,
                slot,
                Vec3::new(plan.x, centre_y, plan.y) + Vec3::new(outward.x, 0.0, outward.y) * 0.12,
                frame_size,
                SolidRole::FrameMember,
                crate::ResolvedSolidShape::Cuboid,
                wall_node,
            ));
        }

        let exterior_depth_sign = if tangent.x.abs() > 0.5 {
            if outward.y >= 0.0 { 1 } else { -1 }
        } else if outward.x <= 0.0 {
            1
        } else {
            -1
        };
        let void_half = tangent.abs() * (opening_width * 0.5) + outward.abs() * (thickness * 0.5);
        let void_id = wall_void(
            geometry,
            owner,
            10,
            ResolvedBounds {
                min: Vec3::new(
                    origin.x - void_half.x,
                    sill_elevation,
                    origin.y - void_half.y,
                ),
                max: Vec3::new(origin.x + void_half.x, head_bottom, origin.y + void_half.y),
            },
            opening_id,
            opening_width,
            opening_width,
            clear_height,
            clear_height,
            exterior_depth_sign,
        );
        let reveal_depth = outward.abs() * (thickness * 0.5);
        let side_half = tangent.abs() * 0.008;
        let mut reveal_surfaces = Vec::new();
        for (slot, side, role) in [
            (10_u64, -1.0_f32, SurfaceRole::LeftJambReveal),
            (11, 1.0, SurfaceRole::RightJambReveal),
        ] {
            let plan = origin + tangent * side * opening_width * 0.5;
            reveal_surfaces.push(wall_surface(
                geometry,
                owner,
                slot,
                ResolvedBounds {
                    min: Vec3::new(
                        plan.x - reveal_depth.x - side_half.x,
                        sill_elevation,
                        plan.y - reveal_depth.y - side_half.y,
                    ),
                    max: Vec3::new(
                        plan.x + reveal_depth.x + side_half.x,
                        head_bottom,
                        plan.y + reveal_depth.y + side_half.y,
                    ),
                },
                role,
            ));
        }
        reveal_surfaces.push(wall_shaped_surface(
            geometry,
            owner,
            12,
            ResolvedBounds {
                min: Vec3::new(
                    origin.x - void_half.x,
                    sill_elevation,
                    origin.y - void_half.y,
                ),
                max: Vec3::new(
                    origin.x + void_half.x,
                    sill_elevation + 0.015,
                    origin.y + void_half.y,
                ),
            },
            SurfaceRole::WeatherSill,
            crate::ResolvedSurfaceShape::WeatherSill {
                interior_elevation_metres: sill_elevation,
                exterior_elevation_metres: sill_elevation - 0.035,
                drip_depth_metres: 0.025,
            },
        ));
        reveal_surfaces.push(wall_surface(
            geometry,
            owner,
            13,
            ResolvedBounds {
                min: Vec3::new(
                    origin.x - void_half.x,
                    head_bottom - 0.015,
                    origin.y - void_half.y,
                ),
                max: Vec3::new(origin.x + void_half.x, head_bottom, origin.y + void_half.y),
            },
            SurfaceRole::Intrados,
        ));
        for (slot, sign, role) in [
            (14_u64, 1.0_f32, SurfaceRole::ExteriorThroat),
            (15, -1.0, SurfaceRole::InteriorMouth),
        ] {
            let plan = origin + outward * thickness * 0.5 * sign;
            let half = tangent.abs() * opening_width * 0.5 + outward.abs() * 0.006;
            reveal_surfaces.push(wall_surface(
                geometry,
                owner,
                slot,
                ResolvedBounds {
                    min: Vec3::new(plan.x - half.x, sill_elevation, plan.y - half.y),
                    max: Vec3::new(plan.x + half.x, head_bottom, plan.y + half.y),
                },
                role,
            ));
        }
        let closure = closure_policy_for(program.archetype, crate::OpeningUse::Window);
        let mut closure_solids = Vec::new();
        for (layer_index, layer) in closure.layers.iter().copied().enumerate() {
            let role = if layer == crate::ClosureKind::LeadedGlazing {
                SolidRole::LeadedGlazing
            } else {
                SolidRole::OpeningClosure
            };
            let plan = origin - outward * (0.065 + layer_index as f32 * 0.035);
            closure_solids.push(wall_solid(
                geometry,
                owner,
                20 + layer_index as u64,
                Vec3::new(plan.x, sill_elevation + clear_height * 0.5, plan.y),
                local_size(
                    (opening_width * 0.92 - 0.10).max(0.04),
                    (clear_height * 0.92 - 0.10).max(0.04),
                    0.025,
                ),
                role,
                crate::ResolvedSolidShape::Cuboid,
                head_node,
            ));
        }
        let bearing_ids = [
            ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | 60),
            ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | 61),
        ];
        for (side, id) in [-1.0_f32, 1.0].into_iter().zip(bearing_ids) {
            let plan = origin + tangent * side * (opening_width * 0.5 + 0.03);
            geometry.support_interfaces.push(SupportInterface {
                id,
                owner,
                node: head_node,
                bounds: ResolvedBounds {
                    min: Vec3::new(plan.x - 0.08, head_bottom, plan.y - 0.08),
                    max: Vec3::new(plan.x + 0.08, head_bottom + 0.08, plan.y + 0.08),
                },
            });
        }
        let wall_above_interface = ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | 62);
        geometry.support_interfaces.push(SupportInterface {
            id: wall_above_interface,
            owner,
            node: spandrel_node,
            bounds: ResolvedBounds {
                min: Vec3::new(
                    origin.x - 0.08,
                    head_bottom + head_height - 0.025,
                    origin.y - 0.08,
                ),
                max: Vec3::new(
                    origin.x + 0.08,
                    head_bottom + head_height + 0.025,
                    origin.y + 0.08,
                ),
            },
        });
        openings.push(crate::OpeningAssembly {
            id: opening_id,
            owner,
            host_wall: wall_id,
            host_source: crate::WallSourceId::RoofChildFront { roof: roof_id },
            frame: crate::WallLocalFrame {
                origin,
                tangent,
                outward,
                inside_room: None,
                outside_room: None,
            },
            use_kind: crate::OpeningUse::Window,
            profile: crate::OpeningProfile::Rectangular {
                width_metres: opening_width,
                height_metres: clear_height,
            },
            sill_elevation_metres: sill_elevation,
            closure,
            head_kind: crate::OpeningHeadKind::TimberLintel,
            void_id,
            jamb_solids,
            sill_solid: Some(sill_solid),
            head_solid,
            spandrel_solid,
            reveal_surfaces,
            closure_solids,
            jamb_nodes,
            head_node,
            spandrel_node,
            tracery_node: None,
            stance_surface: None,
            mount_solid: None,
            ray_indices: Vec::new(),
            sectional_void: (0..=8)
                .map(|slice| crate::OpeningVoidSlice {
                    depth_fraction: slice as f32 / 8.0,
                    width_metres: opening_width,
                    height_metres: clear_height,
                })
                .collect(),
            head_bearing_interfaces: bearing_ids,
            wall_above_interface,
        });
        walls.push(crate::WallAssembly {
            id: wall_id,
            owner,
            source: crate::WallSourceId::RoofChildFront { roof: roof_id },
            material: crate::WallMaterialClass::TimberInfill,
            storey_level: program.storeys.len() as u16,
            frame: crate::WallLocalFrame {
                origin,
                tangent,
                outward,
                inside_room: None,
                outside_room: None,
            },
            radial_frame: None,
            length_metres: width,
            height_metres: height,
            base_elevation_metres: base,
            thickness_metres: thickness,
            structural_role: crate::WallStructuralRole::LoadBearing,
            support_node: wall_node,
            host_solids,
            opening_ids: vec![opening_id],
            replaced_by_owner: None,
        });
        for (bond_slot, roof_owner) in parent_owner.into_iter().enumerate() {
            geometry.junction_bonds.push(JunctionBond {
                id: ResolvedItemId(
                    (0x6_u64 << 60) | (u64::from(owner.0) << 16) | (1 + bond_slot as u64),
                ),
                owners: [roof_owner, owner],
                bounds: ResolvedBounds {
                    min: Vec3::new(
                        origin.x - tangent.x.abs() * width * 0.55 - outward.x.abs() * 0.30,
                        base - 0.12,
                        origin.y - tangent.y.abs() * width * 0.55 - outward.y.abs() * 0.30,
                    ),
                    max: Vec3::new(
                        origin.x + tangent.x.abs() * width * 0.55 + outward.x.abs() * 0.30,
                        top + 0.18,
                        origin.y + tangent.y.abs() * width * 0.55 + outward.y.abs() * 0.30,
                    ),
                },
                minimum_interface_area_square_metres: 0.005,
                maximum_penetration_metres: 0.18,
            });
        }
        if dormer.kind == DormerKind::TransverseGable
            && let (Some(parent_id), Some((facade_id, _))) = (child.parent, facade_wall)
            && let Some(parent) = roofs.iter_mut().find(|roof| roof.id == parent_id)
            && let Some(link) = parent
                .children
                .iter_mut()
                .find(|link| link.child == roof_id)
        {
            link.facade_wall = Some(facade_id);
        }
    }
}

fn resolve_round_tower_wall_assemblies(
    towers: &[RoundTower],
    crowns: &[CrownAssembly],
    walls: &mut Vec<crate::WallAssembly>,
    geometry: &mut ResolvedGeometry,
) {
    for (tower_index, tower) in towers.iter().copied().enumerate() {
        let serial = walls.len() as u64 + 1;
        let id = crate::WallAssemblyId(serial);
        let owner = GeometryOwnerId(60_000 + tower_index as u32);
        let support_node = StructuralNodeId(3_000_000 + tower_index as u64);
        let centre = tower.centre_metres();
        geometry.structural_nodes.push(StructuralNode {
            id: support_node,
            owner,
            kind: StructuralNodeKind::WallBearing,
            position: Vec3::new(centre.x, 0.0, centre.y),
            supported_by: Vec::new(),
            grounded: true,
        });
        let host = wall_solid(
            geometry,
            owner,
            0,
            Vec3::new(centre.x, tower.wall_height_metres * 0.5, centre.y),
            Vec3::new(
                tower.radius_metres() * 2.0,
                tower.wall_height_metres,
                tower.radius_metres() * 2.0,
            ),
            SolidRole::WallHost,
            crate::ResolvedSolidShape::RoundTowerShell {
                outer_radius_metres: tower.radius_metres(),
                inner_radius_metres: tower.radius_metres() - tower.wall_thickness_metres,
                chord_interfaces: [tower.chord_interface, tower.secondary_chord_interface],
            },
            support_node,
        );
        walls.push(crate::WallAssembly {
            id,
            owner,
            source: crate::WallSourceId::RoundTower { tower_index },
            material: crate::WallMaterialClass::FortifiedMasonry,
            storey_level: 0,
            frame: crate::WallLocalFrame {
                origin: centre,
                tangent: Vec2::X,
                outward: -Vec2::Y,
                inside_room: None,
                outside_room: None,
            },
            radial_frame: Some(crate::RadialWallFrame {
                centre,
                reference_outward: -Vec2::Y,
            }),
            length_metres: std::f32::consts::TAU * tower.radius_metres(),
            height_metres: tower.wall_height_metres,
            base_elevation_metres: 0.0,
            thickness_metres: tower.wall_thickness_metres,
            structural_role: crate::WallStructuralRole::TowerShell,
            support_node,
            host_solids: vec![host],
            opening_ids: Vec::new(),
            replaced_by_owner: None,
        });
        if let Some(crown) = crowns.iter().find(|crown| {
            matches!(
                crown.path,
                CrownPath::Round { tower_index: index, .. } if index == tower_index
            )
        }) {
            geometry.junction_bonds.push(JunctionBond {
                id: ResolvedItemId((7_u64 << 60) | (u64::from(owner.0) << 24) | tower_index as u64),
                owners: [owner, crown.owner],
                bounds: ResolvedBounds {
                    min: Vec3::new(
                        centre.x - tower.radius_metres() - 0.05,
                        tower.wall_height_metres - 0.08,
                        centre.y - tower.radius_metres() - 0.05,
                    ),
                    max: Vec3::new(
                        centre.x + tower.radius_metres() + 0.05,
                        tower.wall_height_metres + 0.18,
                        centre.y + tower.radius_metres() + 0.05,
                    ),
                },
                minimum_interface_area_square_metres: 0.01,
                // The resolved annular shell's conservative AABB overlaps the
                // full radial depth of its segmented deck. The physical
                // interface remains the tower-top annulus recorded above.
                maximum_penetration_metres: 1.10,
            });
        }
    }
}

/// Resolve the single artillery-adapted MVP as one authority.  The exact
/// dimensions below are project animation/gameplay gates, not universal
/// measurements for sixteenth-century fortifications.
fn resolve_artillery_castle(
    program: &BuildingProgram,
    towers: &[RoundTower],
    walls: &mut Vec<crate::WallAssembly>,
    openings: &mut Vec<crate::OpeningAssembly>,
    geometry: &mut ResolvedGeometry,
) -> Option<crate::ArtilleryCastleAssembly> {
    if program.archetype != BuildingArchetype::ArtilleryRondelCastle {
        return None;
    }
    let trace = [
        crate::GridPoint::new(-240, -180),
        crate::GridPoint::new(480, -180),
        crate::GridPoint::new(480, 420),
        crate::GridPoint::new(-240, 420),
    ];
    let crown = 6.0_f32;
    let total_depth = crate::GridLength::new(90).expect("4.5m artillery curtain depth");
    let mut curtains = Vec::new();
    let mut support_ids = Vec::new();
    let mut artillery_drainage_routes = Vec::new();
    let curtain_specs = [
        (crate::Direction::South, Vec2::new(6.0, -9.0), 34.8_f32),
        (crate::Direction::East, Vec2::new(24.0, 6.0), 28.8),
        (crate::Direction::North, Vec2::new(6.0, 21.0), 34.8),
        (crate::Direction::West, Vec2::new(-12.0, 6.0), 28.8),
    ];
    for (index, (direction, inner_mid, length)) in curtain_specs.into_iter().enumerate() {
        let owner = GeometryOwnerId(80_000 + index as u32);
        let outward = direction_vector(direction);
        let tangent = Vec2::new(-outward.y, outward.x);
        let revetment_node = StructuralNodeId(40_000_000 + index as u64 * 4);
        let retaining_node = StructuralNodeId(revetment_node.0 + 1);
        let terreplein_node = StructuralNodeId(revetment_node.0 + 2);
        geometry.structural_nodes.extend([
            StructuralNode {
                id: revetment_node,
                owner,
                kind: StructuralNodeKind::ArtilleryRevetmentBearing,
                position: Vec3::new(
                    inner_mid.x + outward.x * 4.05,
                    0.0,
                    inner_mid.y + outward.y * 4.05,
                ),
                supported_by: Vec::new(),
                grounded: true,
            },
            StructuralNode {
                id: retaining_node,
                owner,
                kind: StructuralNodeKind::ArtilleryRetainingBearing,
                position: Vec3::new(
                    inner_mid.x + outward.x * 0.25,
                    0.0,
                    inner_mid.y + outward.y * 0.25,
                ),
                supported_by: Vec::new(),
                grounded: true,
            },
            StructuralNode {
                id: terreplein_node,
                owner,
                kind: StructuralNodeKind::ArtilleryTerrepleinBearing,
                position: Vec3::new(
                    inner_mid.x + outward.x * 2.25,
                    5.55,
                    inner_mid.y + outward.y * 2.25,
                ),
                supported_by: vec![revetment_node, retaining_node],
                grounded: false,
            },
        ]);
        let rev_plan = inner_mid + outward * 4.05;
        let earth_plan = inner_mid + outward * 2.25;
        let retain_plan = inner_mid + outward * 0.25;
        let split_layer = |geometry: &mut ResolvedGeometry,
                           plan: Vec2,
                           depth: f32,
                           height: f32,
                           role: SolidRole,
                           supports: Vec<StructuralNodeId>| {
            if direction == crate::Direction::South {
                [-3.5_f32, 15.5]
                    .into_iter()
                    .map(|x| {
                        projected_solid(
                            geometry,
                            owner,
                            Vec3::new(x, height * 0.5, plan.y),
                            Vec3::new(15.8, height, depth),
                            0.0,
                            role,
                            supports.clone(),
                        )
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![projected_solid(
                    geometry,
                    owner,
                    Vec3::new(plan.x, height * 0.5, plan.y),
                    if tangent.x.abs() > 0.5 {
                        Vec3::new(length, height, depth)
                    } else {
                        Vec3::new(depth, height, length)
                    },
                    0.0,
                    role,
                    supports,
                )]
            }
        };
        let revetments = split_layer(
            geometry,
            rev_plan,
            0.9,
            crown,
            SolidRole::ArtilleryRevetment,
            vec![revetment_node],
        );
        let earths = split_layer(
            geometry,
            earth_plan,
            3.1,
            5.5,
            SolidRole::ArtilleryEarthCore,
            vec![revetment_node, retaining_node],
        );
        let retainings = split_layer(
            geometry,
            retain_plan,
            0.5,
            crown,
            SolidRole::ArtilleryRetainingWall,
            vec![retaining_node],
        );
        let deck_plan = inner_mid + outward * 1.95;
        let terreplein = projected_solid(
            geometry,
            owner,
            Vec3::new(deck_plan.x, 5.74, deck_plan.y),
            if tangent.x.abs() > 0.5 {
                Vec3::new(length, 0.22, 3.10)
            } else {
                Vec3::new(3.10, 0.22, length)
            },
            0.0,
            SolidRole::ArtilleryTerreplein,
            vec![terreplein_node],
        );
        let yaw = -tangent.y.atan2(tangent.x);
        let local_positive_z = Vec2::new(yaw.sin(), yaw.cos());
        if let Some(deck) = geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == terreplein)
        {
            deck.yaw_radians = yaw;
            deck.size = Vec3::new(length, 0.22, 3.10);
            deck.crossfall_radians = 0.025 * outward.dot(local_positive_z).signum();
        }
        let parapet_plan = inner_mid + outward * 4.02;
        let parapet = projected_solid(
            geometry,
            owner,
            Vec3::new(parapet_plan.x, 6.65, parapet_plan.y),
            if tangent.x.abs() > 0.5 {
                Vec3::new(length, 1.3, 0.95)
            } else {
                Vec3::new(0.95, 1.3, length)
            },
            0.0,
            SolidRole::ArtilleryParapet,
            vec![revetment_node],
        );
        if let Some(solid) = geometry.solids.iter_mut().find(|solid| solid.id == parapet) {
            if tangent.x.abs() > 0.5 {
                solid.size.x -= 1.8;
            } else {
                solid.size.z -= 1.8;
            }
        }
        let route_surface = projected_surface(
            geometry,
            owner,
            ResolvedBounds {
                min: Vec3::new(
                    deck_plan.x - tangent.x.abs() * length * 0.5 - outward.x.abs() * 1.25,
                    5.85,
                    deck_plan.y - tangent.y.abs() * length * 0.5 - outward.y.abs() * 1.25,
                ),
                max: Vec3::new(
                    deck_plan.x + tangent.x.abs() * length * 0.5 + outward.x.abs() * 1.25,
                    5.88,
                    deck_plan.y + tangent.y.abs() * length * 0.5 + outward.y.abs() * 1.25,
                ),
            },
            SurfaceRole::ArtilleryRoute,
        );
        let catchment = projected_surface(
            geometry,
            owner,
            ResolvedBounds {
                min: Vec3::new(
                    deck_plan.x - tangent.x.abs() * length * 0.5 - outward.x.abs() * 1.65,
                    5.84,
                    deck_plan.y - tangent.y.abs() * length * 0.5 - outward.y.abs() * 1.65,
                ),
                max: Vec3::new(
                    deck_plan.x + tangent.x.abs() * length * 0.5 + outward.x.abs() * 1.65,
                    5.87,
                    deck_plan.y + tangent.y.abs() * length * 0.5 + outward.y.abs() * 1.65,
                ),
            },
            SurfaceRole::ArtilleryDrainage,
        );
        let channel_plan = inner_mid + outward * 3.55;
        let channel = projected_solid(
            geometry,
            owner,
            Vec3::new(channel_plan.x, 5.595, channel_plan.y),
            Vec3::new(length, 0.05, 0.10),
            yaw,
            SolidRole::DrainageFloor,
            vec![terreplein_node],
        );
        geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == channel)
            .expect("artillery curtain gutter")
            .longfall_radians = 0.010;
        let inlet_plan = channel_plan - tangent * (length * 0.5 - 0.08);
        let route = projected_edge_drain(
            geometry,
            owner,
            Vec3::new(inlet_plan.x, 5.57, inlet_plan.y),
            outward,
        );
        geometry.drainage_catchments.push(DrainageCatchment {
            id: catchment,
            owner,
            walk_solid: terreplein,
            toe_channel_solids: vec![channel],
            drainage_surface: catchment,
            outlet_route: route,
            centre: Vec3::new(deck_plan.x, 5.85, deck_plan.y),
            tangent,
            outward,
            length_metres: length,
            width_metres: 3.10,
            inner_elevation_metres: 5.88,
            outer_elevation_metres: 5.82,
            outlet_along_metres: -length * 0.5 + 0.08,
        });
        artillery_drainage_routes.push(route);
        let (inner_start, inner_end) = if tangent.x.abs() > 0.5 {
            (
                crate::GridPoint::new(-240, (inner_mid.y / crate::GRID_UNIT_METRES) as i32),
                crate::GridPoint::new(480, (inner_mid.y / crate::GRID_UNIT_METRES) as i32),
            )
        } else {
            (
                crate::GridPoint::new((inner_mid.x / crate::GRID_UNIT_METRES) as i32, -180),
                crate::GridPoint::new((inner_mid.x / crate::GRID_UNIT_METRES) as i32, 420),
            )
        };
        curtains.push(crate::ArtilleryCurtainAssembly {
            id: crate::ArtilleryCurtainId(index as u64),
            owner,
            outward: direction,
            inner_start,
            inner_end,
            total_depth,
            height_metres: crown,
            revetment_solids: revetments,
            earth_solids: earths,
            retaining_solids: retainings,
            terreplein_solid: terreplein,
            parapet_solid: parapet,
            route_surface,
            drainage_catchment: catchment,
            drainage_route: route,
            suppressed_source_walls: Vec::new(),
        });
        support_ids.extend(
            geometry
                .support_interfaces
                .iter()
                .rev()
                .take(5)
                .map(|interface| interface.id),
        );
    }

    let mut rondels = Vec::new();
    let mut stations = Vec::new();
    for (index, tower) in towers.iter().take(4).enumerate() {
        let owner = GeometryOwnerId(60_000 + index as u32);
        let centre = tower.centre_metres();
        let bearing = StructuralNodeId(41_000_000 + index as u64 * 3);
        let deck_node = StructuralNodeId(bearing.0 + 1);
        geometry.structural_nodes.extend([
            StructuralNode {
                id: bearing,
                owner,
                kind: StructuralNodeKind::ArtilleryRondelBearing,
                position: Vec3::new(centre.x, 0.0, centre.y),
                supported_by: Vec::new(),
                grounded: true,
            },
            StructuralNode {
                id: deck_node,
                owner,
                kind: StructuralNodeKind::ArtilleryTerrepleinBearing,
                position: Vec3::new(centre.x, 5.55, centre.y),
                supported_by: vec![bearing],
                grounded: false,
            },
        ]);
        let casemate_void = projected_void(
            geometry,
            owner,
            ResolvedBounds {
                min: Vec3::new(centre.x - 2.5, 0.20, centre.y - 2.5),
                max: Vec3::new(centre.x + 2.5, 2.75, centre.y + 2.5),
            },
            VoidRole::ArtilleryCasemate,
        );
        let floor = projected_solid(
            geometry,
            owner,
            Vec3::new(centre.x, 0.10, centre.y),
            Vec3::new(5.0, 0.20, 5.0),
            0.0,
            SolidRole::ArtilleryCasemateFloor,
            vec![bearing],
        );
        let roof = projected_solid(
            geometry,
            owner,
            Vec3::new(centre.x, 2.90, centre.y),
            Vec3::new(5.2, 0.30, 5.2),
            0.0,
            SolidRole::ArtilleryCasemateRoof,
            vec![bearing],
        );
        geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == roof)
            .unwrap()
            .shape = crate::ResolvedSolidShape::AnnularPrism {
            inner_radius_metres: 1.10,
            outer_radius_metres: 2.60,
            inner_top_offset_metres: 0.0,
            outer_top_offset_metres: 0.0,
            drainage_outlet_count: 0,
            circumferential_fall_metres: 0.0,
        };
        // The low battery is a genuine earth-backed rondel. The residual
        // sectors are resolved after the station working volumes exist, so
        // the actual port, stance, mount, recoil, vent, and access authority
        // determines every omission rather than a nominal angular slot.
        let inward = Vec2::new(
            if index % 2 == 0 { 1.0 } else { -1.0 },
            if index < 2 { 1.0 } else { -1.0 },
        );
        let mut earths = Vec::new();
        let terreplein = projected_solid(
            geometry,
            owner,
            Vec3::new(centre.x, 5.72, centre.y),
            Vec3::new(9.60, 0.24, 9.60),
            0.0,
            SolidRole::ArtilleryTerreplein,
            vec![deck_node],
        );
        geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == terreplein)
            .expect("rondel annular deck")
            .shape = crate::ResolvedSolidShape::AnnularPrism {
            inner_radius_metres: 1.10,
            outer_radius_metres: 4.80,
            inner_top_offset_metres: 0.035,
            outer_top_offset_metres: -0.035,
            drainage_outlet_count: 4,
            circumferential_fall_metres: 0.025,
        };
        let route = projected_surface(
            geometry,
            owner,
            ResolvedBounds {
                min: Vec3::new(centre.x - 3.5, 5.84, centre.y - 3.5),
                max: Vec3::new(centre.x + 3.5, 5.87, centre.y + 3.5),
            },
            SurfaceRole::ArtilleryRoute,
        );
        let mut rondel_drainage = Vec::new();
        for (drain_index, outward) in [Vec2::X, Vec2::Y, Vec2::NEG_X, Vec2::NEG_Y]
            .into_iter()
            .enumerate()
        {
            let tangent = Vec2::new(-outward.y, outward.x);
            let channel_plan = centre + outward * 4.83;
            let channels = [-1.0_f32, 1.0]
                .into_iter()
                .map(|side| {
                    let plan = channel_plan + tangent * side * 0.46;
                    let channel = projected_solid(
                        geometry,
                        owner,
                        Vec3::new(plan.x, 5.645, plan.y),
                        Vec3::new(0.92, 0.05, 0.10),
                        -tangent.y.atan2(tangent.x),
                        SolidRole::DrainageFloor,
                        vec![deck_node],
                    );
                    geometry
                        .solids
                        .iter_mut()
                        .find(|solid| solid.id == channel)
                        .expect("rondel V gutter half")
                        .longfall_radians = side * 0.015;
                    channel
                })
                .collect::<Vec<_>>();
            let drain_surface = projected_surface(
                geometry,
                owner,
                ResolvedBounds {
                    min: Vec3::new(centre.x - 4.8, 5.64, centre.y - 4.8),
                    max: Vec3::new(centre.x + 4.8, 5.88, centre.y + 4.8),
                },
                SurfaceRole::ArtilleryDrainage,
            );
            let route_id = projected_edge_drain(
                geometry,
                owner,
                Vec3::new(channel_plan.x, 5.61, channel_plan.y),
                outward,
            );
            geometry.drainage_catchments.push(DrainageCatchment {
                id: drain_surface,
                owner,
                walk_solid: terreplein,
                toe_channel_solids: channels,
                drainage_surface: drain_surface,
                outlet_route: route_id,
                centre: Vec3::new(centre.x, 5.84, centre.y),
                tangent,
                outward,
                length_metres: std::f32::consts::PI * 4.8 * 0.5,
                width_metres: 3.7,
                inner_elevation_metres: 5.875,
                outer_elevation_metres: 5.805,
                outlet_along_metres: drain_index as f32,
            });
            rondel_drainage.push(route_id);
            artillery_drainage_routes.push(route_id);
        }
        let adjoining = match index {
            0 => [crate::ArtilleryCurtainId(0), crate::ArtilleryCurtainId(3)],
            1 => [crate::ArtilleryCurtainId(0), crate::ArtilleryCurtainId(1)],
            2 => [crate::ArtilleryCurtainId(2), crate::ArtilleryCurtainId(3)],
            _ => [crate::ArtilleryCurtainId(2), crate::ArtilleryCurtainId(1)],
        };
        let mut bonds = [ResolvedItemId::default(); 2];
        for bond_index in 0..2 {
            let interface = tower
                .chord_interfaces()
                .nth(bond_index)
                .expect("two artillery returns");
            let toward = direction_vector(interface.toward_gate);
            let bond_centre =
                centre + toward * (tower.radius_metres() - interface.bearing_depth.metres() * 0.5);
            let id = ResolvedItemId((7_u64 << 60) | (u64::from(owner.0) << 24) | bond_index as u64);
            geometry.junction_bonds.push(JunctionBond {
                id,
                owners: [owner, curtains[adjoining[bond_index].0 as usize].owner],
                bounds: ResolvedBounds {
                    min: Vec3::new(bond_centre.x - 1.25, 0.0, bond_centre.y - 1.25),
                    max: Vec3::new(bond_centre.x + 1.25, crown + 0.25, bond_centre.y + 1.25),
                },
                minimum_interface_area_square_metres: 0.25,
                maximum_penetration_metres: 1.25,
            });
            bonds[bond_index] = id;
        }
        let mut station_ids = Vec::new();
        let outward_y = if index < 2 { -1.0 } else { 1.0 };
        // The covered batteries fire tangentially along the two adjoining
        // curtain feet. The open upper position covers the outward ditch.
        let facings = [
            Vec2::new(inward.x, 0.0),
            Vec2::new(0.0, inward.y),
            Vec2::new(0.0, outward_y),
        ];
        for (station_index, facing) in facings.into_iter().enumerate() {
            let level = if station_index < 2 {
                crate::ArtilleryStationLevel::LowerCasemate
            } else {
                crate::ArtilleryStationLevel::UpperTerreplein
            };
            let opening_id =
                crate::OpeningAssemblyId(90_000 + index as u64 * 3 + station_index as u64);
            let wall_index = walls.iter().position(|wall| matches!(wall.source, crate::WallSourceId::RoundTower { tower_index } if tower_index == index)).expect("artillery rondel radial host");
            let mut station_wall = walls[wall_index].clone();
            station_wall.id =
                crate::WallAssemblyId(90_000 + index as u64 * 3 + station_index as u64);
            station_wall.source = crate::WallSourceId::ArtilleryRondel {
                rondel_index: index,
                station_index,
            };
            station_wall.owner = GeometryOwnerId(83_000 + (index * 3 + station_index) as u32);
            station_wall.length_metres = 1.50;
            station_wall.radial_frame = Some(crate::RadialWallFrame {
                centre: tower.centre_metres(),
                reference_outward: facing,
            });
            station_wall.opening_ids.clear();
            station_wall.host_solids.clear();
            let station = resolve_artillery_gun_opening(
                index,
                station_index,
                facing,
                level,
                opening_id,
                &mut station_wall,
                openings,
                geometry,
            );
            let resolved_opening = openings.last().expect("artillery opening");
            station_wall.host_solids = resolved_opening
                .jamb_solids
                .iter()
                .copied()
                .chain([resolved_opening.head_solid, resolved_opening.spandrel_solid])
                .collect();
            walls.push(station_wall);
            let aperture_origin = openings.last().unwrap().frame.origin;
            geometry.junction_bonds.push(JunctionBond {
                id: ResolvedItemId(
                    (7_u64 << 60) | (u64::from(owner.0) << 20) | (0x100 + station_index as u64),
                ),
                owners: [
                    owner,
                    GeometryOwnerId(83_000 + (index * 3 + station_index) as u32),
                ],
                bounds: ResolvedBounds {
                    min: Vec3::new(
                        aperture_origin.x - 0.9,
                        floor_for_artillery_level(level),
                        aperture_origin.y - 0.9,
                    ),
                    max: Vec3::new(
                        aperture_origin.x + 0.9,
                        floor_for_artillery_level(level) + 2.5,
                        aperture_origin.y + 0.9,
                    ),
                },
                minimum_interface_area_square_metres: 0.05,
                maximum_penetration_metres: 1.25,
            });
            if station_index < 2 {
                geometry.junction_bonds.push(JunctionBond {
                    id: ResolvedItemId(
                        (7_u64 << 60) | (u64::from(owner.0) << 20) | (0x180 + station_index as u64),
                    ),
                    owners: [
                        curtains[adjoining[station_index].0 as usize].owner,
                        GeometryOwnerId(83_000 + (index * 3 + station_index) as u32),
                    ],
                    bounds: ResolvedBounds {
                        min: Vec3::new(
                            aperture_origin.x - 1.25,
                            floor_for_artillery_level(level),
                            aperture_origin.y - 1.25,
                        ),
                        max: Vec3::new(
                            aperture_origin.x + 1.25,
                            floor_for_artillery_level(level) + 2.55,
                            aperture_origin.y + 1.25,
                        ),
                    },
                    minimum_interface_area_square_metres: 0.08,
                    maximum_penetration_metres: 1.25,
                });
            }
            station_ids.push(station.id);
            stations.push(station);
        }
        let mut earth_clearances = vec![
            geometry
                .voids
                .iter()
                .find(|void| void.id == casemate_void)
                .expect("rondel casemate void")
                .bounds,
        ];
        for station in stations.iter().filter(|station| {
            station.rondel == crate::ArtilleryRondelId(index as u64)
                && station.level == crate::ArtilleryStationLevel::LowerCasemate
        }) {
            earth_clearances.push(station.recoil_envelope);
            if let Some(stance) = geometry
                .surfaces
                .iter()
                .find(|surface| surface.id == station.stance_surface)
            {
                earth_clearances.push(ResolvedBounds {
                    min: stance.bounds.min - Vec3::new(0.02, 0.0, 0.02),
                    max: stance.bounds.max + Vec3::new(0.02, 1.90, 0.02),
                });
            }
            if let Some(mount) = geometry
                .solids
                .iter()
                .find(|solid| solid.id == station.mount_solid)
            {
                earth_clearances.push(ResolvedBounds {
                    min: mount.centre - mount.size * 0.5,
                    max: mount.centre + mount.size * 0.5,
                });
            }
            if let Some(opening) = openings
                .iter()
                .find(|opening| opening.id == station.opening)
                && let Some(void) = geometry
                    .voids
                    .iter()
                    .find(|void| void.id == opening.void_id)
            {
                earth_clearances.push(void.bounds);
            }
            if let Some(vent) = station.smoke_vent
                && let Some(void) = geometry.voids.iter().find(|void| void.id == vent)
            {
                earth_clearances.push(void.bounds);
            }
        }
        let sector_bounds = |start: f32, end: f32| {
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            for angle in [start, (start + end) * 0.5, end] {
                for radius in [3.60_f32, 4.775] {
                    let point = Vec3::new(
                        centre.x + radius * angle.cos(),
                        0.0,
                        centre.y + radius * angle.sin(),
                    );
                    min = min.min(point);
                    max = max.max(point + Vec3::Y * 5.50);
                }
            }
            ResolvedBounds { min, max }
        };
        for sector in 0..32 {
            let start = sector as f32 * std::f32::consts::TAU / 32.0;
            let end = (sector + 1) as f32 * std::f32::consts::TAU / 32.0;
            let bounds = sector_bounds(start, end);
            let reserved = earth_clearances.iter().any(|clearance| {
                bounds.max.x.min(clearance.max.x) - bounds.min.x.max(clearance.min.x) > 0.005
                    && bounds.max.y.min(clearance.max.y) - bounds.min.y.max(clearance.min.y) > 0.005
                    && bounds.max.z.min(clearance.max.z) - bounds.min.z.max(clearance.min.z) > 0.005
            });
            if reserved {
                continue;
            }
            let id = projected_solid(
                geometry,
                owner,
                Vec3::new(centre.x, 2.75, centre.y),
                Vec3::new(9.55, 5.50, 9.55),
                0.0,
                SolidRole::ArtilleryEarthCore,
                vec![bearing],
            );
            geometry
                .solids
                .iter_mut()
                .find(|solid| solid.id == id)
                .expect("rondel residual earth sector")
                .shape = crate::ResolvedSolidShape::AnnularSectorPrism {
                inner_radius_metres: 3.60,
                outer_radius_metres: 4.775,
                start_angle_radians: start,
                end_angle_radians: end,
                inner_top_offset_metres: 0.0,
                outer_top_offset_metres: 0.0,
            };
            earths.push(id);
        }
        let shell = walls.iter().find(|wall| matches!(wall.source, crate::WallSourceId::RoundTower { tower_index } if tower_index == index)).and_then(|wall| wall.host_solids.first()).copied().expect("rondel shell");
        let mut stair_solids = Vec::new();
        let stair_arrival_angle = inward.y.atan2(inward.x).rem_euclid(std::f32::consts::TAU);
        for tread in 0..32_u16 {
            let progress = f32::from(tread) / 31.0;
            let angle = stair_arrival_angle + progress * std::f32::consts::TAU * 2.0;
            let radial = Vec2::new(angle.cos(), angle.sin());
            let tread_plan = centre + radial * 0.65;
            let tread_solid = projected_solid(
                geometry,
                owner,
                Vec3::new(tread_plan.x, 0.22 + progress * 5.58, tread_plan.y),
                Vec3::new(0.90, 0.12, 0.38),
                -radial.y.atan2(radial.x),
                SolidRole::ArtilleryStairTread,
                vec![bearing],
            );
            stair_solids.push(tread_solid);
        }
        let mut parapet_solids = Vec::new();
        let access_angles = tower
            .chord_interfaces()
            .map(|interface| {
                let toward = direction_vector(interface.toward_gate);
                toward.y.atan2(toward.x).rem_euclid(std::f32::consts::TAU)
            })
            .collect::<Vec<_>>();
        let firing_angle = Vec2::new(0.0, outward_y)
            .y
            .atan2(0.0_f32)
            .rem_euclid(std::f32::consts::TAU);
        for sector in 0..32 {
            let start = sector as f32 * std::f32::consts::TAU / 32.0;
            let end = (sector + 1) as f32 * std::f32::consts::TAU / 32.0;
            let middle = (start + end) * 0.5;
            let angular_distance = |angle: f32| {
                (middle - angle)
                    .rem_euclid(std::f32::consts::TAU)
                    .min((angle - middle).rem_euclid(std::f32::consts::TAU))
            };
            if access_angles
                .iter()
                .any(|angle| angular_distance(*angle) < 0.50)
                || angular_distance(firing_angle) < 0.14
            {
                continue;
            }
            let id = projected_solid(
                geometry,
                owner,
                Vec3::new(centre.x, 6.475, centre.y),
                Vec3::new(11.7, 1.25, 11.7),
                0.0,
                SolidRole::ArtilleryParapet,
                vec![deck_node],
            );
            geometry
                .solids
                .iter_mut()
                .find(|solid| solid.id == id)
                .unwrap()
                .shape = crate::ResolvedSolidShape::AnnularSectorPrism {
                inner_radius_metres: 5.00,
                outer_radius_metres: 5.85,
                start_angle_radians: start,
                end_angle_radians: end,
                inner_top_offset_metres: 0.04,
                outer_top_offset_metres: -0.04,
            };
            parapet_solids.push(id);
        }
        let mut stair_guard_solids = Vec::new();
        let guard_sector_angle = std::f32::consts::TAU / 32.0;
        let arrival_half_angle = (0.45_f32 / 1.30).asin();
        for sector in 0..32 {
            let start = sector as f32 * guard_sector_angle;
            let end = (sector + 1) as f32 * guard_sector_angle;
            let middle = (start + end) * 0.5;
            let arrival_distance = (middle - stair_arrival_angle)
                .rem_euclid(std::f32::consts::TAU)
                .min((stair_arrival_angle - middle).rem_euclid(std::f32::consts::TAU));
            // The 1.10m stair well gets a continuous 0.95m guard. The only
            // omitted sectors are exactly those intersecting the positive-
            // width 0.90 m occupant sweep at the authoritative tread arrival.
            // Half a sector is included because this discrete authority omits
            // complete annular sectors, never partial visual-only wedges.
            if arrival_distance < arrival_half_angle + guard_sector_angle * 0.5 {
                continue;
            }
            let id = projected_solid(
                geometry,
                owner,
                Vec3::new(centre.x, 6.335, centre.y),
                Vec3::new(2.86, 0.95, 2.86),
                0.0,
                SolidRole::ArtilleryStairGuard,
                vec![deck_node],
            );
            geometry
                .solids
                .iter_mut()
                .find(|solid| solid.id == id)
                .expect("rondel stair-well guard")
                .shape = crate::ResolvedSolidShape::AnnularSectorPrism {
                inner_radius_metres: 1.30,
                outer_radius_metres: 1.43,
                start_angle_radians: start,
                end_angle_radians: end,
                inner_top_offset_metres: 0.0,
                outer_top_offset_metres: -0.02,
            };
            stair_guard_solids.push(id);
        }
        rondels.push(crate::ArtilleryRondelAssembly {
            id: crate::ArtilleryRondelId(index as u64),
            owner,
            anchor: tower.anchor(),
            diameter: tower.diameter(),
            shell: crate::GridLength::new(
                (tower.wall_thickness_metres / crate::GRID_UNIT_METRES).round() as i32,
            )
            .expect("shell"),
            adjoining_curtains: adjoining,
            curtain_bonds: bonds,
            shell_solid: shell,
            earth_solids: earths,
            casemate_void,
            casemate_floor: floor,
            casemate_roof: roof,
            terreplein_solid: terreplein,
            parapet_solids,
            stair_guard_solids,
            route_surfaces: vec![route],
            stair_solids,
            drainage_routes: rondel_drainage,
            station_ids,
            support_nodes: vec![bearing, deck_node],
        });
    }

    // Stable tactical targets turn the firing proof into a coverage matrix.
    // Auxiliary targets preserve each station's near/middle/far calibration;
    // required targets sample every curtain foot, ditch corner, and approach.
    let mut defense_targets = Vec::new();
    let mut next_target = 0_u64;
    for station in &mut stations {
        for ray in &mut station.rays {
            let id = crate::ArtilleryTargetId(next_target);
            next_target += 1;
            ray.target_id = id;
            defense_targets.push(crate::ArtilleryDefenseTarget {
                id,
                kind: ray.target_kind,
                centre: ray.target,
                half_extent_metres: Vec2::splat(0.35),
                required_independent_stations: 0,
            });
        }
    }
    let mut required = Vec::new();
    for (kind, points, independent) in [
        (
            crate::ArtilleryTargetKind::CurtainFoot,
            vec![
                Vec3::new(-8.0, 0.2, -13.5),
                Vec3::new(6.0, 0.2, -13.5),
                Vec3::new(20.0, 0.2, -13.5),
                Vec3::new(28.5, 0.2, -4.0),
                Vec3::new(28.5, 0.2, 6.0),
                Vec3::new(28.5, 0.2, 16.0),
                Vec3::new(-8.0, 0.2, 25.5),
                Vec3::new(6.0, 0.2, 25.5),
                Vec3::new(20.0, 0.2, 25.5),
                Vec3::new(-16.5, 0.2, -4.0),
                Vec3::new(-16.5, 0.2, 6.0),
                Vec3::new(-16.5, 0.2, 16.0),
            ],
            2_u8,
        ),
        (
            crate::ArtilleryTargetKind::DitchCorner,
            vec![
                Vec3::new(-20.0, -1.0, -17.0),
                Vec3::new(32.0, -1.0, -17.0),
                Vec3::new(-20.0, -1.0, 29.0),
                Vec3::new(32.0, -1.0, 29.0),
            ],
            1,
        ),
        (
            crate::ArtilleryTargetKind::GateThreshold,
            vec![Vec3::new(6.0, 0.2, -13.5)],
            2,
        ),
        (
            crate::ArtilleryTargetKind::Bridge,
            vec![Vec3::new(6.0, 0.2, -17.0)],
            2,
        ),
        (
            crate::ArtilleryTargetKind::Approach,
            vec![Vec3::new(6.0, 0.2, -25.0)],
            2,
        ),
    ] {
        for point in points {
            let id = crate::ArtilleryTargetId(next_target);
            next_target += 1;
            defense_targets.push(crate::ArtilleryDefenseTarget {
                id,
                kind,
                centre: point,
                half_extent_metres: Vec2::splat(0.45),
                required_independent_stations: independent,
            });
            required.push(id);
        }
    }
    for target_id in required {
        let target = defense_targets
            .iter()
            .find(|target| target.id == target_id)
            .unwrap()
            .clone();
        let mut candidates = stations
            .iter()
            .enumerate()
            .filter_map(|(index, station)| {
                let origin = station.rays.first()?.origin;
                let delta = Vec2::new(target.centre.x - origin.x, target.centre.z - origin.z);
                let distance = delta.length();
                (distance > 2.0
                    && station.facing.dot(delta / distance) >= 38.0_f32.to_radians().cos() - 0.01)
                    .then_some((distance, index))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (distance, index) in candidates
            .into_iter()
            .take(target.required_independent_stations as usize)
        {
            let origin = stations[index].rays[0].origin;
            let range = if distance < 8.0 {
                crate::ProjectedDefenseRange::Near
            } else if distance < 18.0 {
                crate::ProjectedDefenseRange::Middle
            } else {
                crate::ProjectedDefenseRange::Far
            };
            stations[index].rays.push(crate::ArtilleryFireRay {
                target_id,
                origin,
                target: target.centre,
                target_kind: target.kind,
                range,
            });
        }
    }

    let ditch_owner = GeometryOwnerId(82_000);
    let ditch_node = StructuralNodeId(42_000_000);
    geometry.structural_nodes.push(StructuralNode {
        id: ditch_node,
        owner: ditch_owner,
        kind: StructuralNodeKind::ArtilleryRevetmentBearing,
        position: Vec3::new(6.0, -2.2, 6.0),
        supported_by: Vec::new(),
        grounded: true,
    });
    let ditch_void = projected_void(
        geometry,
        ditch_owner,
        ResolvedBounds {
            min: Vec3::new(-22.5, -2.19, -19.5),
            max: Vec3::new(34.5, 0.0, 31.5),
        },
        VoidRole::DryDitch,
    );
    geometry
        .voids
        .iter_mut()
        .find(|void| void.id == ditch_void)
        .unwrap()
        .shape = crate::ResolvedVoidShape::RectangularRing {
        inner_min: Vec2::new(-16.7, -13.5),
        inner_max: Vec2::new(28.7, 25.5),
    };
    let mut floors = Vec::new();
    for (centre, tangent, length) in [
        (Vec3::new(6.0, -2.3, -17.0), Vec2::X, 57.0_f32),
        (Vec3::new(6.0, -2.3, 29.0), Vec2::X, 57.0),
        (Vec3::new(-20.0, -2.3, 6.0), Vec2::Y, 41.0),
        (Vec3::new(32.0, -2.3, 6.0), Vec2::Y, 41.0),
    ] {
        let floor = projected_solid(
            geometry,
            ditch_owner,
            centre,
            Vec3::new(length, 0.20, 5.0),
            -tangent.y.atan2(tangent.x),
            SolidRole::DitchFloor,
            vec![ditch_node],
        );
        geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == floor)
            .expect("sloped ditch floor")
            .longfall_radians = 0.004;
        floors.push(floor);
    }
    let mut scarp_solids = Vec::new();
    let mut counterscarp_solids = Vec::new();
    for (centre, size, yaw) in [
        // The south scarp is split around the grounded bridge abutment.
        (
            Vec3::new(-6.10, -1.15, -13.67),
            Vec3::new(20.80, 2.30, 0.30),
            0.0,
        ),
        (
            Vec3::new(18.10, -1.15, -13.67),
            Vec3::new(20.80, 2.30, 0.30),
            0.0,
        ),
        (
            Vec3::new(6.0, -1.15, 25.67),
            Vec3::new(45.0, 2.30, 0.30),
            0.0,
        ),
        (
            Vec3::new(-16.67, -1.15, 6.0),
            Vec3::new(0.30, 2.30, 34.0),
            0.0,
        ),
        (
            Vec3::new(28.67, -1.15, 6.0),
            Vec3::new(0.30, 2.30, 34.0),
            0.0,
        ),
    ] {
        scarp_solids.push(projected_solid(
            geometry,
            ditch_owner,
            centre,
            size,
            yaw,
            SolidRole::DitchScarp,
            vec![ditch_node],
        ));
    }
    for (centre, size, yaw) in [
        (
            Vec3::new(-9.10, -1.15, -19.35),
            Vec3::new(26.80, 2.30, 0.30),
            0.0,
        ),
        (
            Vec3::new(21.10, -1.15, -19.35),
            Vec3::new(26.80, 2.30, 0.30),
            0.0,
        ),
        (
            Vec3::new(6.0, -1.15, 31.35),
            Vec3::new(57.0, 2.30, 0.35),
            0.0,
        ),
        (
            Vec3::new(-22.35, -1.15, 6.0),
            Vec3::new(0.35, 2.30, 51.0),
            0.0,
        ),
        (
            Vec3::new(34.35, -1.15, 6.0),
            Vec3::new(0.35, 2.30, 51.0),
            0.0,
        ),
    ] {
        counterscarp_solids.push(projected_solid(
            geometry,
            ditch_owner,
            centre,
            size,
            yaw,
            SolidRole::DitchCounterscarp,
            vec![ditch_node],
        ));
    }
    let ditch_outlet = projected_surface(
        geometry,
        ditch_owner,
        ResolvedBounds {
            min: Vec3::new(31.5, -2.31, 28.5),
            max: Vec3::new(32.5, -2.27, 29.5),
        },
        SurfaceRole::DitchSplash,
    );
    let ditch_drain =
        projected_edge_drain(geometry, ditch_owner, Vec3::new(32.0, -2.42, 29.0), Vec2::X);
    let bridge_owner = GeometryOwnerId(82_100);
    let bridge_node = StructuralNodeId(42_100_000);
    geometry.structural_nodes.push(StructuralNode {
        id: bridge_node,
        owner: bridge_owner,
        kind: StructuralNodeKind::ArtilleryBridgeAbutment,
        position: Vec3::new(6.0, 0.0, -17.0),
        supported_by: Vec::new(),
        grounded: true,
    });
    let inner_abutment = projected_solid(
        geometry,
        bridge_owner,
        Vec3::new(6.0, -0.1, -14.2),
        Vec3::new(3.2, 1.0, 1.2),
        0.0,
        SolidRole::ArtilleryBridgeAbutment,
        vec![bridge_node],
    );
    let outer_abutment = projected_solid(
        geometry,
        bridge_owner,
        Vec3::new(6.0, -0.1, -19.8),
        Vec3::new(3.2, 1.0, 1.2),
        0.0,
        SolidRole::ArtilleryBridgeAbutment,
        vec![bridge_node],
    );
    let fixed = projected_solid(
        geometry,
        bridge_owner,
        Vec3::new(6.0, 0.18, -15.9),
        Vec3::new(2.4, 0.28, 2.4),
        0.0,
        SolidRole::ArtilleryBridgeDeck,
        vec![bridge_node],
    );
    let bridge_state = if program.seed % 1_000 == 702 {
        crate::BridgeState::Denied
    } else {
        crate::BridgeState::Deployed
    };
    let removable = projected_solid(
        geometry,
        bridge_owner,
        Vec3::new(6.0, 0.18, -18.10),
        Vec3::new(2.4, 0.28, 2.10),
        0.0,
        SolidRole::ArtilleryBridgeDeck,
        vec![bridge_node],
    );
    let denied_gap = (bridge_state == crate::BridgeState::Denied).then(|| {
        projected_void(
            geometry,
            bridge_owner,
            ResolvedBounds {
                min: Vec3::new(4.8, 0.0, -19.15),
                max: Vec3::new(7.2, 2.0, -17.4),
            },
            VoidRole::BridgeDeniedGap,
        )
    });
    if bridge_state == crate::BridgeState::Denied {
        geometry.solids.retain(|solid| solid.id != removable);
    }
    let bridge_route = (bridge_state == crate::BridgeState::Deployed).then(|| {
        projected_surface(
            geometry,
            bridge_owner,
            ResolvedBounds {
                min: Vec3::new(4.9, 0.32, -19.0),
                max: Vec3::new(7.1, 0.35, -13.6),
            },
            SurfaceRole::ArtilleryRoute,
        )
    });
    let controls = [
        projected_surface(
            geometry,
            bridge_owner,
            ResolvedBounds {
                min: Vec3::new(4.7, 0.0, -13.5),
                max: Vec3::new(5.7, 0.03, -12.5),
            },
            SurfaceRole::ArtilleryStance,
        ),
        projected_surface(
            geometry,
            bridge_owner,
            ResolvedBounds {
                min: Vec3::new(6.3, 0.0, -13.5),
                max: Vec3::new(7.3, 0.03, -12.5),
            },
            SurfaceRole::ArtilleryStance,
        ),
    ];
    let gate_owner = curtains[0].owner;
    let gate_void = projected_void(
        geometry,
        gate_owner,
        ResolvedBounds {
            min: Vec3::new(4.4, 0.0, -13.55),
            max: Vec3::new(7.6, 3.6, -8.95),
        },
        VoidRole::Passage,
    );
    let gate_node = StructuralNodeId(42_200_000);
    geometry.structural_nodes.push(StructuralNode {
        id: gate_node,
        owner: gate_owner,
        kind: StructuralNodeKind::OpeningJamb,
        position: Vec3::new(6.0, 0.0, -9.0),
        supported_by: Vec::new(),
        grounded: true,
    });
    let chamber_node = StructuralNodeId(42_200_001);
    geometry.structural_nodes.push(StructuralNode {
        id: chamber_node,
        owner: gate_owner,
        kind: StructuralNodeKind::ArtilleryTerrepleinBearing,
        position: Vec3::new(6.0, 5.58, -10.65),
        supported_by: vec![gate_node],
        grounded: false,
    });
    let gate_leaf = projected_solid(
        geometry,
        gate_owner,
        Vec3::new(6.0, 1.7, -9.2),
        Vec3::new(3.0, 3.4, 0.12),
        0.0,
        SolidRole::OpeningClosure,
        vec![gate_node],
    );
    let portcullis = projected_solid(
        geometry,
        gate_owner,
        Vec3::new(6.0, 1.8, -10.1),
        Vec3::new(3.0, 3.6, 0.10),
        0.0,
        SolidRole::OpeningClosure,
        vec![gate_node],
    );
    let gate_chamber_solids = vec![
        projected_solid(
            geometry,
            gate_owner,
            Vec3::new(6.0, 5.70, -10.65),
            Vec3::new(3.6, 0.24, 3.0),
            0.0,
            SolidRole::ArtilleryCasemateFloor,
            vec![chamber_node],
        ),
        projected_solid(
            geometry,
            gate_owner,
            Vec3::new(6.0, 8.02, -10.65),
            Vec3::new(3.6, 0.24, 3.0),
            0.0,
            SolidRole::ArtilleryCasemateRoof,
            vec![chamber_node],
        ),
        projected_solid(
            geometry,
            gate_owner,
            Vec3::new(4.35, 6.86, -10.65),
            Vec3::new(0.30, 2.10, 3.0),
            0.0,
            SolidRole::ArtilleryRetainingWall,
            vec![chamber_node],
        ),
        projected_solid(
            geometry,
            gate_owner,
            Vec3::new(7.65, 6.86, -10.65),
            Vec3::new(0.30, 2.10, 3.0),
            0.0,
            SolidRole::ArtilleryRetainingWall,
            vec![chamber_node],
        ),
        projected_solid(
            geometry,
            gate_owner,
            Vec3::new(4.95, 6.86, -12.0),
            Vec3::new(0.90, 2.10, 0.30),
            0.0,
            SolidRole::ArtilleryRetainingWall,
            vec![chamber_node],
        ),
        projected_solid(
            geometry,
            gate_owner,
            Vec3::new(7.05, 6.86, -12.0),
            Vec3::new(0.90, 2.10, 0.30),
            0.0,
            SolidRole::ArtilleryRetainingWall,
            vec![chamber_node],
        ),
        projected_solid(
            geometry,
            gate_owner,
            Vec3::new(5.05, 6.32, -10.35),
            Vec3::new(1.60, 0.22, 0.22),
            0.0,
            SolidRole::ArtilleryGateMechanism,
            vec![chamber_node],
        ),
        projected_solid(
            geometry,
            gate_owner,
            Vec3::new(4.22, 4.75, -10.35),
            Vec3::new(0.08, 2.95, 0.08),
            0.0,
            SolidRole::ArtilleryGateMechanism,
            vec![chamber_node],
        ),
    ];
    let operator = projected_surface(
        geometry,
        gate_owner,
        ResolvedBounds {
            min: Vec3::new(4.65, 5.83, -11.7),
            max: Vec3::new(7.35, 5.86, -9.4),
        },
        SurfaceRole::ArtilleryStance,
    );
    let mut route_nodes = Vec::new();
    let mut route_edges = Vec::new();
    let mut next_route = 0_u64;
    let mut add_route_node = |surface, position, nodes: &mut Vec<crate::ArtilleryRouteNode>| {
        let id = crate::ArtilleryRouteNodeId(next_route);
        next_route += 1;
        nodes.push(crate::ArtilleryRouteNode {
            id,
            surface,
            position,
        });
        id
    };
    let outer_approach = bridge_route
        .map(|surface| add_route_node(surface, Vec3::new(6.0, 0.34, -18.6), &mut route_nodes));
    let gate_outer = add_route_node(controls[0], Vec3::new(5.2, 0.34, -13.0), &mut route_nodes);
    let gate_inner = add_route_node(controls[1], Vec3::new(6.8, 0.02, -7.8), &mut route_nodes);
    if let Some(outer) = outer_approach {
        route_edges.push(crate::ArtilleryRouteEdge {
            from: outer,
            to: gate_outer,
            width_metres: 1.8,
            headroom_metres: 2.1,
            portal_void: None,
            traversal_surface: None,
            connector_solids: vec![fixed, removable],
            sweep_path: Vec::new(),
        });
    }
    route_edges.push(crate::ArtilleryRouteEdge {
        from: gate_outer,
        to: gate_inner,
        width_metres: 0.9,
        headroom_metres: 2.1,
        portal_void: Some(gate_void),
        traversal_surface: None,
        connector_solids: Vec::new(),
        sweep_path: Vec::new(),
    });
    let ramp_owner = GeometryOwnerId(82_300);
    let ramp_node = StructuralNodeId(42_300_000);
    let retaining = retaining_support_node(&curtains, geometry);
    geometry.structural_nodes.push(StructuralNode {
        id: ramp_node,
        owner: ramp_owner,
        kind: StructuralNodeKind::ArtilleryTerrepleinBearing,
        position: Vec3::new(20.5, 0.0, -5.0),
        supported_by: vec![retaining],
        grounded: false,
    });
    let ramp = projected_solid(
        geometry,
        ramp_owner,
        Vec3::new(20.5, 2.9, 6.0),
        Vec3::new(22.0, 0.28, 2.2),
        -std::f32::consts::FRAC_PI_2,
        SolidRole::ArtilleryRamp,
        vec![ramp_node],
    );
    if let Some(solid) = geometry.solids.iter_mut().find(|solid| solid.id == ramp) {
        solid.longfall_radians = (5.8_f32 / 22.0).atan();
    }
    let court_surface = projected_surface(
        geometry,
        ramp_owner,
        ResolvedBounds {
            min: Vec3::new(4.0, 0.0, -7.5),
            max: Vec3::new(8.0, 0.03, -3.5),
        },
        SurfaceRole::ArtilleryRoute,
    );
    let ramp_bottom = projected_surface(
        geometry,
        ramp_owner,
        ResolvedBounds {
            min: Vec3::new(19.4, 0.0, -5.0),
            max: Vec3::new(21.6, 0.03, -2.8),
        },
        SurfaceRole::ArtilleryRoute,
    );
    let ramp_top = projected_surface(
        geometry,
        ramp_owner,
        ResolvedBounds {
            min: Vec3::new(19.4, 5.8, 14.8),
            max: Vec3::new(21.6, 5.83, 17.0),
        },
        SurfaceRole::ArtilleryRoute,
    );
    // Cut the inner retaining wall at the protected ramp landing.  The ramp
    // reaches the terreplein through this real 2 m portal rather than a
    // semantic edge through intact masonry.
    let ramp_portal = projected_void(
        geometry,
        curtains[1].owner,
        ResolvedBounds {
            min: Vec3::new(23.65, 6.00, 14.85),
            max: Vec3::new(27.50, 8.10, 16.95),
        },
        VoidRole::AccessPortal,
    );
    for layer in 0..2 {
        let old_id = if layer == 0 {
            curtains[1].retaining_solids[0]
        } else {
            curtains[1].earth_solids[0]
        };
        if let Some(old) = geometry
            .solids
            .iter()
            .find(|solid| solid.id == old_id)
            .cloned()
        {
            let original_min = old.centre.z - old.size.z * 0.5;
            let original_max = old.centre.z + old.size.z * 0.5;
            let south_length = 14.85 - original_min;
            let north_length = original_max - 16.95;
            if let Some(south) = geometry.solids.iter_mut().find(|solid| solid.id == old_id) {
                south.centre.z = original_min + south_length * 0.5;
                south.size.z = south_length;
            }
            let north = projected_solid(
                geometry,
                old.owner,
                Vec3::new(old.centre.x, old.centre.y, 16.95 + north_length * 0.5),
                Vec3::new(old.size.x, old.size.y, north_length),
                old.yaw_radians,
                old.role,
                old.supported_by,
            );
            if layer == 0 {
                curtains[1].retaining_solids = vec![old_id, north];
            } else {
                curtains[1].earth_solids = vec![old_id, north];
            }
        }
    }
    let court_id = add_route_node(court_surface, Vec3::new(6.0, 0.02, -5.5), &mut route_nodes);
    route_edges.push(crate::ArtilleryRouteEdge {
        from: gate_inner,
        to: court_id,
        width_metres: 1.8,
        headroom_metres: 2.2,
        portal_void: None,
        traversal_surface: None,
        connector_solids: Vec::new(),
        sweep_path: Vec::new(),
    });
    let ramp_bottom_id = add_route_node(ramp_bottom, Vec3::new(20.5, 0.02, -3.9), &mut route_nodes);
    let ramp_top_id = add_route_node(ramp_top, Vec3::new(20.5, 5.82, 15.9), &mut route_nodes);
    route_edges.extend([
        crate::ArtilleryRouteEdge {
            from: court_id,
            to: ramp_bottom_id,
            width_metres: 2.0,
            headroom_metres: 2.2,
            portal_void: None,
            traversal_surface: None,
            connector_solids: Vec::new(),
            sweep_path: Vec::new(),
        },
        crate::ArtilleryRouteEdge {
            from: ramp_bottom_id,
            to: ramp_top_id,
            width_metres: 2.0,
            headroom_metres: 2.1,
            portal_void: None,
            traversal_surface: None,
            connector_solids: vec![ramp],
            sweep_path: Vec::new(),
        },
    ]);
    let mut curtain_nodes = Vec::new();
    for curtain in &curtains {
        let surface = geometry
            .surfaces
            .iter()
            .find(|surface| surface.id == curtain.route_surface)
            .unwrap();
        let mut position = (surface.bounds.min + surface.bounds.max) * 0.5;
        if curtain.id == crate::ArtilleryCurtainId(0) {
            position.x = 10.5;
        }
        curtain_nodes.push(add_route_node(
            curtain.route_surface,
            position,
            &mut route_nodes,
        ));
    }
    route_edges.push(crate::ArtilleryRouteEdge {
        from: ramp_top_id,
        to: curtain_nodes[1],
        width_metres: 2.0,
        headroom_metres: 2.1,
        portal_void: Some(ramp_portal),
        traversal_surface: None,
        connector_solids: Vec::new(),
        sweep_path: Vec::new(),
    });
    for index in 0..4 {
        route_edges.push(crate::ArtilleryRouteEdge {
            from: curtain_nodes[index],
            to: curtain_nodes[(index + 1) % 4],
            width_metres: 2.0,
            headroom_metres: 2.1,
            portal_void: None,
            traversal_surface: None,
            connector_solids: Vec::new(),
            sweep_path: Vec::new(),
        });
    }
    for (rondel_index, rondel) in rondels.iter().enumerate() {
        let surface = rondel.route_surfaces[0];
        let centre = rondel.anchor.metres();
        let curtain_index = rondel
            .adjoining_curtains
            .into_iter()
            .map(|id| id.0 as usize)
            .min_by(|left, right| {
                let lp = route_nodes
                    .iter()
                    .find(|node| node.id == curtain_nodes[*left])
                    .unwrap()
                    .position;
                let rp = route_nodes
                    .iter()
                    .find(|node| node.id == curtain_nodes[*right])
                    .unwrap()
                    .position;
                Vec2::new(lp.x, lp.z)
                    .distance(centre)
                    .total_cmp(&Vec2::new(rp.x, rp.z).distance(centre))
            })
            .unwrap();
        let curtain_position = route_nodes
            .iter()
            .find(|node| node.id == curtain_nodes[curtain_index])
            .unwrap()
            .position;
        let toward = towers[rondel_index]
            .chord_interfaces()
            .map(|interface| direction_vector(interface.toward_gate))
            .min_by(|left, right| {
                (centre + *left * 5.0 - Vec2::new(curtain_position.x, curtain_position.z))
                    .length()
                    .total_cmp(
                        &(centre + *right * 5.0
                            - Vec2::new(curtain_position.x, curtain_position.z))
                        .length(),
                    )
            })
            .unwrap();
        let position = Vec3::new(centre.x + toward.x * 3.5, 5.86, centre.y + toward.y * 3.5);
        let upper = add_route_node(surface, position, &mut route_nodes);
        let portal = Vec3::new(centre.x + toward.x * 5.1, 5.86, centre.y + toward.y * 5.1);
        let pre = if curtain_index == 0 || curtain_index == 2 {
            Vec3::new(
                portal.x - (portal.x - curtain_position.x).signum() * 0.8,
                5.86,
                curtain_position.z,
            )
        } else {
            Vec3::new(
                curtain_position.x,
                5.86,
                portal.z - (portal.z - curtain_position.z).signum() * 0.8,
            )
        };
        route_edges.push(crate::ArtilleryRouteEdge {
            from: curtain_nodes[curtain_index],
            to: upper,
            width_metres: 2.0,
            headroom_metres: 2.1,
            portal_void: None,
            traversal_surface: None,
            connector_solids: Vec::new(),
            sweep_path: vec![curtain_position, pre, portal, position],
        });
        let stair_arrival = Vec2::new(
            if rondel_index % 2 == 0 { 1.0 } else { -1.0 },
            if rondel_index < 2 { 1.0 } else { -1.0 },
        )
        .normalize();
        for station in stations.iter().filter(|station| {
            station.rondel == crate::ArtilleryRondelId(rondel_index as u64)
                && station.level == crate::ArtilleryStationLevel::LowerCasemate
        }) {
            let lower_position = geometry
                .surfaces
                .iter()
                .find(|item| item.id == station.stance_surface)
                .map(|item| (item.bounds.min + item.bounds.max) * 0.5)
                .unwrap();
            let lower = add_route_node(station.stance_surface, lower_position, &mut route_nodes);
            let mut stair_path = vec![
                position,
                Vec3::new(
                    centre.x + stair_arrival.x * 1.65,
                    5.86,
                    centre.y + stair_arrival.y * 1.65,
                ),
            ];
            stair_path.extend(rondel.stair_solids.iter().rev().filter_map(|id| {
                geometry
                    .solids
                    .iter()
                    .find(|solid| solid.id == *id)
                    .map(|solid| solid.centre)
            }));
            stair_path.push(lower_position);
            route_edges.push(crate::ArtilleryRouteEdge {
                from: upper,
                to: lower,
                width_metres: 0.9,
                headroom_metres: 2.1,
                portal_void: Some(rondel.casemate_void),
                traversal_surface: None,
                connector_solids: rondel.stair_solids.clone(),
                sweep_path: stair_path,
            });
        }
    }
    let operator_id = add_route_node(operator, Vec3::new(6.0, 5.84, -10.5), &mut route_nodes);
    let curtain_operator_start = route_nodes
        .iter()
        .find(|node| node.id == curtain_nodes[0])
        .unwrap()
        .position;
    route_edges.push(crate::ArtilleryRouteEdge {
        from: curtain_nodes[0],
        to: operator_id,
        width_metres: 0.9,
        headroom_metres: 2.0,
        portal_void: None,
        traversal_surface: None,
        connector_solids: Vec::new(),
        sweep_path: vec![
            curtain_operator_start,
            Vec3::new(8.35, 5.84, -8.35),
            Vec3::new(6.8, 5.84, -8.35),
            Vec3::new(6.0, 5.84, -10.5),
        ],
    });
    let second_control = add_route_node(controls[1], Vec3::new(6.8, 0.02, -13.0), &mut route_nodes);
    route_edges.push(crate::ArtilleryRouteEdge {
        from: gate_outer,
        to: second_control,
        width_metres: 0.9,
        headroom_metres: 2.0,
        portal_void: None,
        traversal_surface: None,
        connector_solids: Vec::new(),
        sweep_path: Vec::new(),
    });
    let route_owner = GeometryOwnerId(82_400);
    for edge in &mut route_edges {
        let from = route_nodes
            .iter()
            .find(|node| node.id == edge.from)
            .unwrap()
            .position;
        let to = route_nodes
            .iter()
            .find(|node| node.id == edge.to)
            .unwrap()
            .position;
        let curtain_pair = curtain_nodes
            .iter()
            .position(|id| *id == edge.from)
            .zip(curtain_nodes.iter().position(|id| *id == edge.to));
        let coarse_path = if !edge.sweep_path.is_empty() {
            edge.sweep_path.clone()
        } else if let Some((left, right)) = curtain_pair {
            let rondel_index = rondels
                .iter()
                .position(|rondel| {
                    rondel
                        .adjoining_curtains
                        .contains(&crate::ArtilleryCurtainId(left as u64))
                        && rondel
                            .adjoining_curtains
                            .contains(&crate::ArtilleryCurtainId(right as u64))
                })
                .unwrap();
            let centre = towers[rondel_index].centre_metres();
            let mut directions = towers[rondel_index]
                .chord_interfaces()
                .map(|interface| direction_vector(interface.toward_gate))
                .collect::<Vec<_>>();
            let from_plan = Vec2::new(from.x, from.z);
            directions.sort_by(|a, b| {
                (centre + *a * 5.0 - from_plan)
                    .length()
                    .total_cmp(&(centre + *b * 5.0 - from_plan).length())
            });
            let start_angle = directions[0].y.atan2(directions[0].x);
            let end_angle = directions[1].y.atan2(directions[1].x);
            let mut sweep = (end_angle - start_angle).rem_euclid(std::f32::consts::TAU);
            if sweep > std::f32::consts::PI {
                sweep -= std::f32::consts::TAU;
            }
            let portal0 = Vec3::new(
                centre.x + directions[0].x * 5.1,
                from.y,
                centre.y + directions[0].y * 5.1,
            );
            let pre0 = if left == 0 || left == 2 {
                Vec3::new(
                    portal0.x - (portal0.x - from.x).signum() * 0.8,
                    from.y,
                    from.z,
                )
            } else {
                Vec3::new(
                    from.x,
                    from.y,
                    portal0.z - (portal0.z - from.z).signum() * 0.8,
                )
            };
            let mut path = vec![from, pre0, portal0];
            path.extend((1..=6).map(|step| {
                let angle = start_angle + sweep * step as f32 / 6.0;
                Vec3::new(
                    centre.x + angle.cos() * 4.35,
                    (from.y + to.y) * 0.5,
                    centre.y + angle.sin() * 4.35,
                )
            }));
            let portal1 = Vec3::new(
                centre.x + directions[1].x * 5.1,
                to.y,
                centre.y + directions[1].y * 5.1,
            );
            path.push(portal1);
            let post1 = if right == 0 || right == 2 {
                Vec3::new(portal1.x - (portal1.x - to.x).signum() * 0.8, to.y, to.z)
            } else {
                Vec3::new(to.x, to.y, portal1.z - (portal1.z - to.z).signum() * 0.8)
            };
            path.push(post1);
            if left == 3 && right == 0 {
                path.extend([
                    Vec3::new(3.2, to.y, -10.95),
                    Vec3::new(3.2, to.y, -7.35),
                    Vec3::new(9.2, to.y, -7.35),
                    Vec3::new(9.2, to.y, -10.95),
                ]);
            }
            path.push(to);
            path
        } else if edge.connector_solids.len() >= 30 {
            let mut points = edge
                .connector_solids
                .iter()
                .filter_map(|id| {
                    geometry
                        .solids
                        .iter()
                        .find(|solid| solid.id == *id)
                        .map(|solid| solid.centre)
                })
                .collect::<Vec<_>>();
            points.sort_by(|a, b| a.y.total_cmp(&b.y));
            if from.y > to.y {
                points.reverse();
            }
            let mut path = vec![from];
            path.extend(points);
            path.push(to);
            path
        } else if (from.y - to.y).abs() < 0.25
            && (from.x - to.x).abs() > 3.0
            && (from.z - to.z).abs() > 3.0
        {
            vec![from, Vec3::new(to.x, (from.y + to.y) * 0.5, from.z), to]
        } else {
            vec![from, to]
        };
        edge.sweep_path = coarse_path
            .windows(2)
            .enumerate()
            .flat_map(|(segment, pair)| {
                let steps = (pair[0].distance(pair[1]) / 0.30).ceil() as usize;
                (0..steps).filter_map(move |step| {
                    ((segment == 0) || step > 0)
                        .then_some(pair[0].lerp(pair[1], step as f32 / steps as f32))
                })
            })
            .chain([to])
            .collect();
        let half = Vec3::new(edge.width_metres * 0.5, 0.03, edge.width_metres * 0.5);
        let id = projected_surface(
            geometry,
            route_owner,
            ResolvedBounds {
                min: from.min(to) - half,
                max: from.max(to) + half,
            },
            SurfaceRole::ArtilleryRoute,
        );
        geometry
            .surfaces
            .iter_mut()
            .find(|surface| surface.id == id)
            .unwrap()
            .shape = crate::ResolvedSurfaceShape::RouteCorridor {
            start: from,
            end: to,
            width_metres: edge.width_metres,
        };
        edge.traversal_surface = Some(id);
    }
    Some(crate::ArtilleryCastleAssembly {
        id: crate::ArtilleryCastleAssemblyId(1),
        phase: crate::CastleConstructionPhase::ArtilleryRetrofit1544,
        trace,
        clear_court_size_metres: Vec2::new(36.0, 30.0),
        crown_elevation_metres: crown,
        curtains,
        rondels,
        stations,
        defense_targets,
        ditch: crate::ArtilleryDitchAssembly {
            width_metres: 5.0,
            depth_metres: 2.3,
            void_id: ditch_void,
            scarp_solids,
            counterscarp_solids,
            floor_solids: floors,
            drainage_routes: vec![ditch_drain],
            outlet_surface: ditch_outlet,
        },
        bridge: crate::ArtilleryBridgeAssembly {
            state: bridge_state,
            clear_width_metres: 2.2,
            inner_abutment,
            outer_abutment,
            fixed_solids: vec![fixed],
            removable_solids: vec![removable],
            denied_gap_void: denied_gap,
            route_surface: bridge_route,
            control_surfaces: controls,
        },
        gate_passage_void: gate_void,
        gate_closure_solids: vec![gate_leaf, portcullis],
        gate_chamber_solids,
        gate_operator_surface: operator,
        service_ramp_solids: vec![ramp],
        route_nodes,
        route_edges,
        retained_keep_setback_metres: 4.5,
        support_interfaces: support_ids,
        drainage_routes: artillery_drainage_routes,
    })
}

fn floor_for_artillery_level(level: crate::ArtilleryStationLevel) -> f32 {
    if level == crate::ArtilleryStationLevel::LowerCasemate {
        0.20
    } else {
        5.86
    }
}

fn retaining_support_node(
    curtains: &[crate::ArtilleryCurtainAssembly],
    geometry: &ResolvedGeometry,
) -> StructuralNodeId {
    geometry
        .structural_nodes
        .iter()
        .find(|node| {
            node.owner == curtains[1].owner
                && node.kind == StructuralNodeKind::ArtilleryRetainingBearing
        })
        .map(|node| node.id)
        .expect("east retaining support")
}

fn resolve_artillery_gun_opening(
    rondel_index: usize,
    station_index: usize,
    facing: Vec2,
    level: crate::ArtilleryStationLevel,
    opening_id: crate::OpeningAssemblyId,
    wall: &mut crate::WallAssembly,
    openings: &mut Vec<crate::OpeningAssembly>,
    geometry: &mut ResolvedGeometry,
) -> crate::ArtilleryFireStation {
    let owner = GeometryOwnerId(83_000 + (rondel_index * 3 + station_index) as u32);
    let tangent = Vec2::new(-facing.y, facing.x);
    let radius = 6.0_f32;
    let centre = wall.radial_frame.unwrap().centre;
    let origin = centre + facing * radius;
    let floor = if level == crate::ArtilleryStationLevel::LowerCasemate {
        0.20
    } else {
        5.86
    };
    let sill = floor
        + if level == crate::ArtilleryStationLevel::LowerCasemate {
            0.82
        } else {
            0.25
        };
    let thickness = wall.thickness_metres;
    let exterior_width = 0.28_f32;
    let interior_width = 1.10_f32;
    let exterior_height = 0.56_f32;
    let interior_height = 1.20_f32;
    let node_base = 43_000_000 + (rondel_index * 24 + station_index * 6) as u64;
    let jamb_nodes = [StructuralNodeId(node_base), StructuralNodeId(node_base + 1)];
    let head_node = StructuralNodeId(node_base + 2);
    let spandrel_node = StructuralNodeId(node_base + 3);
    for (side, node) in [-1.0_f32, 1.0].into_iter().zip(jamb_nodes) {
        geometry.structural_nodes.push(StructuralNode {
            id: node,
            owner,
            kind: StructuralNodeKind::OpeningJamb,
            position: Vec3::new(
                origin.x + tangent.x * side * interior_width * 0.5,
                floor,
                origin.y + tangent.y * side * interior_width * 0.5,
            ),
            supported_by: vec![wall.support_node],
            grounded: false,
        });
    }
    geometry.structural_nodes.extend([
        StructuralNode {
            id: head_node,
            owner,
            kind: StructuralNodeKind::OpeningHead,
            position: Vec3::new(origin.x, sill + interior_height, origin.y),
            supported_by: jamb_nodes.to_vec(),
            grounded: false,
        },
        StructuralNode {
            id: spandrel_node,
            owner,
            kind: StructuralNodeKind::OpeningSpandrel,
            position: Vec3::new(origin.x, sill + interior_height + 0.2, origin.y),
            supported_by: vec![head_node],
            grounded: false,
        },
    ]);
    wall.frame = crate::WallLocalFrame {
        origin,
        tangent,
        outward: facing,
        inside_room: None,
        outside_room: None,
    };
    wall.base_elevation_metres = floor;
    wall.height_metres = 2.45;
    let yaw = -tangent.y.atan2(tangent.x);
    let side_width = (wall.length_metres - exterior_width) * 0.5;
    let jamb = [-1.0_f32, 1.0]
        .into_iter()
        .enumerate()
        .map(|(index, side)| {
            let p = origin + tangent * side * (exterior_width + side_width) * 0.5;
            let id = wall_solid(
                geometry,
                owner,
                index as u64,
                Vec3::new(p.x, floor + wall.height_metres * 0.5, p.y),
                Vec3::new(side_width, wall.height_metres, thickness),
                SolidRole::OpeningJamb,
                crate::ResolvedSolidShape::SplayedReveal {
                    exterior_width_metres: exterior_width,
                    interior_width_metres: interior_width,
                    side: if side < 0.0 { -1 } else { 1 },
                    exterior_depth_sign: if tangent.x.abs() > 0.5 {
                        if facing.y >= 0.0 { 1 } else { -1 }
                    } else if facing.x <= 0.0 {
                        1
                    } else {
                        -1
                    },
                },
                if side < 0.0 {
                    jamb_nodes[0]
                } else {
                    jamb_nodes[1]
                },
            );
            geometry
                .solids
                .iter_mut()
                .find(|solid| solid.id == id)
                .unwrap()
                .yaw_radians = yaw;
            id
        })
        .collect::<Vec<_>>();
    let jamb = [jamb[0], jamb[1]];
    let depth_sign = if tangent.x.abs() > 0.5 {
        if facing.y >= 0.0 { 1 } else { -1 }
    } else if facing.x <= 0.0 {
        1
    } else {
        -1
    };
    let head_bottom = sill + exterior_height;
    let head_top = sill + interior_height + 0.20;
    let head = wall_solid(
        geometry,
        owner,
        2,
        Vec3::new(origin.x, (head_bottom + head_top) * 0.5, origin.y),
        Vec3::new(interior_width + 0.20, head_top - head_bottom, thickness),
        SolidRole::OpeningHead,
        crate::ResolvedSolidShape::SplayedHead {
            exterior_clear_height_metres: exterior_height,
            interior_clear_height_metres: interior_height,
            exterior_depth_sign: depth_sign,
        },
        head_node,
    );
    let spandrel = wall_solid(
        geometry,
        owner,
        3,
        Vec3::new(origin.x, head_top + 0.10, origin.y),
        Vec3::new(interior_width + 0.20, 0.24, thickness),
        SolidRole::OpeningSpandrel,
        crate::ResolvedSolidShape::Cuboid,
        spandrel_node,
    );
    for id in [head, spandrel] {
        geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == id)
            .unwrap()
            .yaw_radians = yaw;
    }
    let half_t = tangent.abs() * interior_width * 0.5;
    let half_d = facing.abs() * thickness * 0.6;
    let void_id = wall_void(
        geometry,
        owner,
        0,
        ResolvedBounds {
            min: Vec3::new(
                origin.x - half_t.x - half_d.x,
                sill,
                origin.y - half_t.y - half_d.y,
            ),
            max: Vec3::new(
                origin.x + half_t.x + half_d.x,
                sill + interior_height,
                origin.y + half_t.y + half_d.y,
            ),
        },
        opening_id,
        exterior_width,
        interior_width,
        exterior_height,
        interior_height,
        depth_sign,
    );
    let left = wall_shaped_surface(
        geometry,
        owner,
        10,
        ResolvedBounds {
            min: Vec3::new(
                origin.x - half_t.x - half_d.x,
                sill,
                origin.y - half_t.y - half_d.y,
            ),
            max: Vec3::new(
                origin.x + half_t.x + half_d.x,
                sill + interior_height,
                origin.y + half_t.y + half_d.y,
            ),
        },
        SurfaceRole::LeftJambReveal,
        crate::ResolvedSurfaceShape::SplayedJamb {
            side: -1,
            exterior_width_metres: exterior_width,
            interior_width_metres: interior_width,
            exterior_depth_sign: 1,
        },
    );
    let right = wall_shaped_surface(
        geometry,
        owner,
        11,
        ResolvedBounds {
            min: Vec3::new(
                origin.x - half_t.x - half_d.x,
                sill,
                origin.y - half_t.y - half_d.y,
            ),
            max: Vec3::new(
                origin.x + half_t.x + half_d.x,
                sill + interior_height,
                origin.y + half_t.y + half_d.y,
            ),
        },
        SurfaceRole::RightJambReveal,
        crate::ResolvedSurfaceShape::SplayedJamb {
            side: 1,
            exterior_width_metres: exterior_width,
            interior_width_metres: interior_width,
            exterior_depth_sign: depth_sign,
        },
    );
    if let Some(surface) = geometry
        .surfaces
        .iter_mut()
        .find(|surface| surface.id == left)
    {
        surface.shape = crate::ResolvedSurfaceShape::SplayedJamb {
            side: -1,
            exterior_width_metres: exterior_width,
            interior_width_metres: interior_width,
            exterior_depth_sign: depth_sign,
        };
    }
    let weather = wall_shaped_surface(
        geometry,
        owner,
        12,
        ResolvedBounds {
            min: Vec3::new(
                origin.x - half_t.x - half_d.x,
                sill - 0.04,
                origin.y - half_t.y - half_d.y,
            ),
            max: Vec3::new(
                origin.x + half_t.x + half_d.x,
                sill + 0.02,
                origin.y + half_t.y + half_d.y,
            ),
        },
        SurfaceRole::WeatherSill,
        crate::ResolvedSurfaceShape::WeatherSill {
            interior_elevation_metres: sill,
            exterior_elevation_metres: sill - 0.04,
            drip_depth_metres: 0.03,
        },
    );
    let intrados = wall_shaped_surface(
        geometry,
        owner,
        13,
        ResolvedBounds {
            min: Vec3::new(
                origin.x - half_t.x - half_d.x,
                sill + exterior_height,
                origin.y - half_t.y - half_d.y,
            ),
            max: Vec3::new(
                origin.x + half_t.x + half_d.x,
                sill + interior_height,
                origin.y + half_t.y + half_d.y,
            ),
        },
        SurfaceRole::Intrados,
        crate::ResolvedSurfaceShape::Planar,
    );
    let exterior_plan = origin + facing * thickness * 0.5;
    let interior_plan = origin - facing * thickness * 0.5;
    let throat = wall_shaped_surface(
        geometry,
        owner,
        14,
        ResolvedBounds {
            min: Vec3::new(
                exterior_plan.x - tangent.x.abs() * exterior_width * 0.5 - facing.x.abs() * 0.006,
                sill,
                exterior_plan.y - tangent.y.abs() * exterior_width * 0.5 - facing.y.abs() * 0.006,
            ),
            max: Vec3::new(
                exterior_plan.x + tangent.x.abs() * exterior_width * 0.5 + facing.x.abs() * 0.006,
                sill + exterior_height,
                exterior_plan.y + tangent.y.abs() * exterior_width * 0.5 + facing.y.abs() * 0.006,
            ),
        },
        SurfaceRole::ExteriorThroat,
        crate::ResolvedSurfaceShape::Planar,
    );
    let mouth = wall_shaped_surface(
        geometry,
        owner,
        15,
        ResolvedBounds {
            min: Vec3::new(
                interior_plan.x - tangent.x.abs() * interior_width * 0.5 - facing.x.abs() * 0.006,
                sill,
                interior_plan.y - tangent.y.abs() * interior_width * 0.5 - facing.y.abs() * 0.006,
            ),
            max: Vec3::new(
                interior_plan.x + tangent.x.abs() * interior_width * 0.5 + facing.x.abs() * 0.006,
                sill + interior_height,
                interior_plan.y + tangent.y.abs() * interior_width * 0.5 + facing.y.abs() * 0.006,
            ),
        },
        SurfaceRole::InteriorMouth,
        crate::ResolvedSurfaceShape::Planar,
    );
    let stance_plan = origin - facing * (thickness * 0.5 + 1.55);
    let stance = projected_surface(
        geometry,
        owner,
        ResolvedBounds {
            min: Vec3::new(stance_plan.x - 0.5, floor, stance_plan.y - 0.5),
            max: Vec3::new(stance_plan.x + 0.5, floor + 0.03, stance_plan.y + 0.5),
        },
        SurfaceRole::ArtilleryStance,
    );
    let mount_pos = origin - facing * (thickness * 0.5 + 0.85);
    let mount = projected_solid(
        geometry,
        owner,
        Vec3::new(mount_pos.x, sill + 0.18, mount_pos.y),
        Vec3::splat(0.22),
        0.0,
        SolidRole::WeaponMount,
        vec![wall.support_node],
    );
    let mut ray_indices = Vec::new();
    let mut rays = Vec::new();
    let eye = Vec3::new(
        origin.x - facing.x * (thickness * 0.5 + 0.02),
        sill + 0.30,
        origin.y - facing.y * (thickness * 0.5 + 0.02),
    );
    for (range, distance) in [
        (ProjectedDefenseRange::Near, 4.0),
        (ProjectedDefenseRange::Middle, 12.0),
        (ProjectedDefenseRange::Far, 24.0),
    ] {
        let southern_gate_flank = rondel_index < 2 && station_index == 0;
        let (target, target_kind) = if southern_gate_flank {
            let (z, kind) = match range {
                ProjectedDefenseRange::Near => (-13.5, crate::ArtilleryTargetKind::GateThreshold),
                ProjectedDefenseRange::Middle => (-17.0, crate::ArtilleryTargetKind::Bridge),
                ProjectedDefenseRange::Far => (-25.0, crate::ArtilleryTargetKind::Approach),
            };
            (Vec3::new(6.0, 0.20, z), kind)
        } else {
            (
                Vec3::new(
                    eye.x + facing.x * distance,
                    0.20,
                    eye.z + facing.y * distance,
                ),
                if station_index < 2 {
                    crate::ArtilleryTargetKind::CurtainFoot
                } else {
                    crate::ArtilleryTargetKind::DitchCorner
                },
            )
        };
        ray_indices.push(geometry.projected_defense_rays.len());
        geometry.projected_defense_rays.push(ProjectedDefenseRay {
            owner,
            throat: void_id,
            stance: Vec3::new(centre.x, floor, centre.y),
            origin: eye,
            target,
            range,
        });
        rays.push(crate::ArtilleryFireRay {
            target_id: crate::ArtilleryTargetId(u64::MAX),
            origin: eye,
            target,
            target_kind,
            range,
        });
    }
    let interfaces = [-1.0_f32, 1.0]
        .into_iter()
        .enumerate()
        .map(|(index, side)| {
            let slot = 50 + index as u64;
            let p = origin + tangent * side * (exterior_width * 0.5 + side_width * 0.5);
            let ext = tangent.abs() * side_width * 0.5 + facing.abs() * thickness * 0.5;
            let id = ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | slot);
            geometry.support_interfaces.push(SupportInterface {
                id,
                owner,
                node: head_node,
                bounds: ResolvedBounds {
                    min: Vec3::new(p.x - ext.x, sill + interior_height - 0.16, p.y - ext.y),
                    max: Vec3::new(p.x + ext.x, sill + interior_height + 0.02, p.y + ext.y),
                },
            });
            id
        })
        .collect::<Vec<_>>();
    let interfaces = [interfaces[0], interfaces[1]];
    let above = ResolvedItemId((4_u64 << 60) | (u64::from(owner.0) << 32) | 52);
    geometry.support_interfaces.push(SupportInterface {
        id: above,
        owner,
        node: spandrel_node,
        bounds: ResolvedBounds {
            min: Vec3::new(
                origin.x - tangent.x.abs() * 0.75 - facing.x.abs() * 0.6,
                sill + interior_height + 0.17,
                origin.y - tangent.y.abs() * 0.75 - facing.y.abs() * 0.6,
            ),
            max: Vec3::new(
                origin.x + tangent.x.abs() * 0.75 + facing.x.abs() * 0.6,
                sill + interior_height + 0.26,
                origin.y + tangent.y.abs() * 0.75 + facing.y.abs() * 0.6,
            ),
        },
    });
    let opening = crate::OpeningAssembly {
        id: opening_id,
        owner,
        host_wall: wall.id,
        host_source: wall.source,
        frame: crate::WallLocalFrame {
            origin,
            tangent,
            outward: facing,
            inside_room: None,
            outside_room: None,
        },
        use_kind: crate::OpeningUse::GunLoop,
        profile: crate::OpeningProfile::GunLoop {
            exterior_width_metres: exterior_width,
            interior_width_metres: interior_width,
            exterior_height_metres: exterior_height,
            interior_height_metres: interior_height,
            mount: crate::WeaponMountClass::LightSwivelGun,
            traverse_degrees: 38.0,
            recoil_metres: 1.8,
            crew_clearance_metres: 2.5,
        },
        sill_elevation_metres: sill,
        closure: crate::ClosurePolicy {
            layers: vec![crate::ClosureKind::OpenMilitary],
            state: crate::ClosureState::Open,
            thickness_metres: 0.0,
            swing_clearance_metres: 0.0,
        },
        head_kind: crate::OpeningHeadKind::StoneLintel,
        void_id,
        jamb_solids: jamb,
        sill_solid: None,
        head_solid: head,
        spandrel_solid: spandrel,
        reveal_surfaces: vec![left, right, weather, intrados, throat, mouth],
        closure_solids: Vec::new(),
        jamb_nodes,
        head_node,
        spandrel_node,
        tracery_node: None,
        stance_surface: Some(stance),
        mount_solid: Some(mount),
        ray_indices,
        sectional_void: (0..=8)
            .map(|i| {
                let t = i as f32 / 8.0;
                crate::OpeningVoidSlice {
                    depth_fraction: t,
                    width_metres: exterior_width + (interior_width - exterior_width) * t,
                    height_metres: exterior_height + (interior_height - exterior_height) * t,
                }
            })
            .collect(),
        head_bearing_interfaces: interfaces,
        wall_above_interface: above,
    };
    wall.opening_ids.push(opening_id);
    openings.push(opening);
    let vent = (level == crate::ArtilleryStationLevel::LowerCasemate).then(|| {
        projected_void(
            geometry,
            owner,
            ResolvedBounds {
                min: Vec3::new(centre.x - 0.15, 2.45, centre.y - 0.15),
                max: Vec3::new(centre.x + 0.15, 3.25, centre.y + 0.15),
            },
            VoidRole::ArtillerySmokeVent,
        )
    });
    let recoil_centre = origin - facing * (thickness * 0.5 + 2.0);
    let recoil_half = facing.abs() * 2.0 + tangent.abs() * 1.25;
    crate::ArtilleryFireStation {
        id: crate::ArtilleryStationId((rondel_index * 3 + station_index) as u64),
        rondel: crate::ArtilleryRondelId(rondel_index as u64),
        level,
        facing,
        opening: opening_id,
        stance_surface: stance,
        mount_solid: mount,
        recoil_envelope: ResolvedBounds {
            min: Vec3::new(
                recoil_centre.x - recoil_half.x,
                floor,
                recoil_centre.y - recoil_half.y,
            ),
            max: Vec3::new(
                recoil_centre.x + recoil_half.x,
                floor + 2.1,
                recoil_centre.y + recoil_half.y,
            ),
        },
        smoke_vent: vent,
        rays,
    }
}

fn replace_storey_wall_sources_inside_round_towers(
    towers: &[RoundTower],
    walls: &mut [crate::WallAssembly],
    openings: &mut Vec<crate::OpeningAssembly>,
    geometry: &mut ResolvedGeometry,
) {
    let round_hosts = walls
        .iter()
        .filter_map(|wall| match wall.source {
            crate::WallSourceId::RoundTower { tower_index } => {
                Some((tower_index, wall.owner, wall.host_solids.first().copied()?))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut removed_owners = std::collections::HashSet::new();
    let mut replaced_wall_ids = std::collections::HashSet::new();
    for wall in walls.iter_mut() {
        if !matches!(wall.source, crate::WallSourceId::StoreyWall { .. }) {
            continue;
        }
        let Some((_, replacement_owner, replacement_host)) =
            round_hosts.iter().find(|(index, _, _)| {
                let tower = towers[*index];
                wall.frame.origin.distance(tower.centre_metres())
                    <= tower.radius_metres() + CELL_SIZE_METRES * 0.5
            })
        else {
            continue;
        };
        removed_owners.insert(wall.owner);
        replaced_wall_ids.insert(wall.id);
        wall.replaced_by_owner = Some(*replacement_owner);
        wall.host_solids = vec![*replacement_host];
        wall.opening_ids.clear();
    }
    let removed_opening_owners = openings
        .iter()
        .filter(|opening| replaced_wall_ids.contains(&opening.host_wall))
        .map(|opening| opening.owner)
        .collect::<std::collections::HashSet<_>>();
    openings.retain(|opening| !replaced_wall_ids.contains(&opening.host_wall));
    let removed = |owner: GeometryOwnerId| {
        removed_owners.contains(&owner) || removed_opening_owners.contains(&owner)
    };
    geometry.solids.retain(|solid| !removed(solid.owner));
    geometry.surfaces.retain(|surface| !removed(surface.owner));
    geometry.voids.retain(|void| !removed(void.owner));
    geometry
        .support_interfaces
        .retain(|interface| !removed(interface.owner));
    geometry
        .junction_bonds
        .retain(|bond| !bond.owners.iter().any(|owner| removed(*owner)));
}

fn resolve_gatehouse_tower_chord_bonds(
    towers: &[RoundTower],
    defenses: &[ProjectedDefenseAssembly],
    walls: &[crate::WallAssembly],
    geometry: &mut ResolvedGeometry,
) {
    for (tower_index, tower) in towers.iter().copied().enumerate() {
        let Some(round_wall) = walls.iter().find(|wall| {
            matches!(
                wall.source,
                crate::WallSourceId::RoundTower { tower_index: index } if index == tower_index
            )
        }) else {
            continue;
        };
        for (interface_index, interface) in tower.chord_interfaces().enumerate() {
            let toward = direction_vector(interface.toward_gate);
            let perpendicular = Vec2::new(-toward.y, toward.x);
            let radius = tower.radius_metres();
            let chord_offset = radius - interface.bearing_depth.metres();
            let half_chord = (radius * radius - chord_offset * chord_offset)
                .max(0.0)
                .sqrt();
            let point = tower.centre_metres() + toward * chord_offset;
            let defense = defenses.iter().find(|defense| {
                let ProjectedDefensePath::Linear { start, end, .. } = defense.path else {
                    return false;
                };
                let delta = end - start;
                let progress =
                    ((point - start).dot(delta) / delta.length_squared()).clamp(0.0, 1.0);
                point.distance(start + delta * progress) <= 0.08
            });
            let Some(defense) = defense else {
                continue;
            };
            let horizontal = toward.abs() * 0.035 + perpendicular.abs() * half_chord;
            for (slot, target_owner) in [defense.host_owner, defense.owner].into_iter().enumerate()
            {
                geometry.junction_bonds.push(JunctionBond {
                    id: ResolvedItemId(
                        (7_u64 << 60)
                            | (u64::from(round_wall.owner.0) << 20)
                            | ((interface_index as u64) << 4)
                            | (slot as u64 + 0x800),
                    ),
                    owners: [round_wall.owner, target_owner],
                    bounds: ResolvedBounds {
                        min: Vec3::new(point.x - horizontal.x, 0.0, point.y - horizontal.y),
                        max: Vec3::new(
                            point.x + horizontal.x,
                            tower.wall_height_metres,
                            point.y + horizontal.y,
                        ),
                    },
                    minimum_interface_area_square_metres: 0.08,
                    maximum_penetration_metres: 0.08,
                });
            }
        }
    }
}

fn resolve_storey_wall_corner_bonds(
    walls: &[crate::WallAssembly],
    geometry: &mut ResolvedGeometry,
) {
    let solids = geometry
        .solids
        .iter()
        .map(|solid| {
            (
                solid.id,
                solid.owner,
                solid.centre,
                solid.size,
                solid.role,
                solid.yaw_radians,
            )
        })
        .collect::<Vec<_>>();
    let mut serial = 0_u64;
    for (left_index, left) in walls.iter().enumerate() {
        for right in &walls[(left_index + 1)..] {
            if left.storey_level != right.storey_level
                || left.owner == right.owner
                || left.frame.tangent.dot(right.frame.tangent).abs() > 0.01
            {
                continue;
            }
            let left_ids = left.replaced_by_owner.map_or_else(
                || left.host_solids.clone(),
                |owner| {
                    solids
                        .iter()
                        .filter_map(|solid| (solid.1 == owner).then_some(solid.0))
                        .collect()
                },
            );
            let right_ids = right.replaced_by_owner.map_or_else(
                || right.host_solids.clone(),
                |owner| {
                    solids
                        .iter()
                        .filter_map(|solid| (solid.1 == owner).then_some(solid.0))
                        .collect()
                },
            );
            for left_id in &left_ids {
                let Some((_, left_owner, left_centre, left_size, left_role, left_yaw)) =
                    solids.iter().find(|solid| solid.0 == *left_id)
                else {
                    continue;
                };
                for right_id in &right_ids {
                    let Some((_, right_owner, right_centre, right_size, right_role, right_yaw)) =
                        solids.iter().find(|solid| solid.0 == *right_id)
                    else {
                        continue;
                    };
                    if !matches!(
                        left_role,
                        SolidRole::WallHost
                            | SolidRole::DefenseHostWall
                            | SolidRole::CircuitWalk
                            | SolidRole::OpeningJamb
                            | SolidRole::OpeningSill
                            | SolidRole::OpeningHead
                    ) || !matches!(
                        right_role,
                        SolidRole::WallHost
                            | SolidRole::DefenseHostWall
                            | SolidRole::CircuitWalk
                            | SolidRole::OpeningJamb
                            | SolidRole::OpeningSill
                            | SolidRole::OpeningHead
                    ) {
                        continue;
                    }
                    let aabb_half = |size: Vec3, yaw: f32| {
                        let cosine = yaw.cos().abs();
                        let sine = yaw.sin().abs();
                        Vec3::new(
                            (size.x * cosine + size.z * sine) * 0.5,
                            size.y * 0.5,
                            (size.x * sine + size.z * cosine) * 0.5,
                        )
                    };
                    let left_half = aabb_half(*left_size, *left_yaw);
                    let right_half = aabb_half(*right_size, *right_yaw);
                    let overlap_min = (*left_centre - left_half).max(*right_centre - right_half);
                    let overlap_max = (*left_centre + left_half).min(*right_centre + right_half);
                    let overlap = overlap_max - overlap_min;
                    if overlap.min_element() <= 0.025 {
                        continue;
                    }
                    let mut extents = [overlap.x, overlap.y, overlap.z];
                    extents.sort_by(f32::total_cmp);
                    geometry.junction_bonds.push(JunctionBond {
                        id: ResolvedItemId((8_u64 << 60) | serial),
                        owners: [*left_owner, *right_owner],
                        bounds: ResolvedBounds {
                            min: overlap_min,
                            max: overlap_max,
                        },
                        minimum_interface_area_square_metres: extents[1] * extents[2] * 0.90,
                        maximum_penetration_metres: overlap.x.min(overlap.z) + 0.005,
                    });
                    serial += 1;
                }
            }
        }
    }
    let wall_owners = walls
        .iter()
        .flat_map(|wall| [wall.owner, wall.replaced_by_owner.unwrap_or(wall.owner)])
        .collect::<HashSet<_>>();
    for (left_index, left) in solids.iter().enumerate() {
        for right in &solids[(left_index + 1)..] {
            if left.1 == right.1
                || (!wall_owners.contains(&left.1) && !wall_owners.contains(&right.1))
                || !matches!(
                    left.4,
                    SolidRole::WallHost
                        | SolidRole::DefenseHostWall
                        | SolidRole::CircuitWalk
                        | SolidRole::LoadBearing
                        | SolidRole::Breastwork
                        | SolidRole::WalkSurface
                        | SolidRole::DrainageChannel
                        | SolidRole::Landing
                        | SolidRole::DefenseHostButtress
                        | SolidRole::ProjectionSupport
                        | SolidRole::GalleryFloor
                        | SolidRole::OpeningJamb
                        | SolidRole::OpeningSill
                        | SolidRole::OpeningHead
                        | SolidRole::OpeningSpandrel
                )
                || !matches!(
                    right.4,
                    SolidRole::WallHost
                        | SolidRole::DefenseHostWall
                        | SolidRole::CircuitWalk
                        | SolidRole::LoadBearing
                        | SolidRole::Breastwork
                        | SolidRole::WalkSurface
                        | SolidRole::DrainageChannel
                        | SolidRole::Landing
                        | SolidRole::DefenseHostButtress
                        | SolidRole::ProjectionSupport
                        | SolidRole::GalleryFloor
                        | SolidRole::OpeningJamb
                        | SolidRole::OpeningSill
                        | SolidRole::OpeningHead
                        | SolidRole::OpeningSpandrel
                )
            {
                continue;
            }
            let aabb_half = |size: Vec3, yaw: f32| {
                let cosine = yaw.cos().abs();
                let sine = yaw.sin().abs();
                Vec3::new(
                    (size.x * cosine + size.z * sine) * 0.5,
                    size.y * 0.5,
                    (size.x * sine + size.z * cosine) * 0.5,
                )
            };
            let left_half = aabb_half(left.3, left.5);
            let right_half = aabb_half(right.3, right.5);
            let overlap_min = (left.2 - left_half).max(right.2 - right_half);
            let overlap_max = (left.2 + left_half).min(right.2 + right_half);
            let overlap = overlap_max - overlap_min;
            if overlap.min_element() <= 0.025
                || geometry.junction_bonds.iter().any(|bond| {
                    bond.owners.contains(&left.1)
                        && bond.owners.contains(&right.1)
                        && overlap_min
                            .cmpge(bond.bounds.min - Vec3::splat(0.025))
                            .all()
                        && overlap_max
                            .cmple(bond.bounds.max + Vec3::splat(0.025))
                            .all()
                })
            {
                continue;
            }
            let mut extents = [overlap.x, overlap.y, overlap.z];
            extents.sort_by(f32::total_cmp);
            geometry.junction_bonds.push(JunctionBond {
                id: ResolvedItemId((8_u64 << 60) | serial),
                owners: [left.1, right.1],
                bounds: ResolvedBounds {
                    min: overlap_min,
                    max: overlap_max,
                },
                minimum_interface_area_square_metres: extents[1] * extents[2] * 0.90,
                maximum_penetration_metres: overlap.x.min(overlap.z) + 0.005,
            });
            serial += 1;
        }
    }
}

fn roof_plane(polygon: &[Vec3]) -> RoofPlaneEquation {
    let mut normal = (polygon[1] - polygon[0])
        .cross(polygon[2] - polygon[0])
        .normalize_or_zero();
    if normal.y < 0.0 {
        normal = -normal;
    }
    RoofPlaneEquation {
        normal,
        constant: -normal.dot(polygon[0]),
    }
}

fn roof_polygon_bounds(polygon: &[Vec3]) -> ResolvedBounds {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for point in polygon {
        min = min.min(*point);
        max = max.max(*point);
    }
    ResolvedBounds { min, max }
}

fn roof_face_polygons(roof: RoofPiece, shed_high_side: Option<Direction>) -> Vec<Vec<Vec3>> {
    let hx = roof.size.x * 0.5 + roof.eave_metres;
    let hz = roof.size.y * 0.5 + roof.eave_metres;
    let y = roof.base_height_metres;
    let corners = [
        Vec3::new(roof.centre.x - hx, y, roof.centre.y - hz),
        Vec3::new(roof.centre.x + hx, y, roof.centre.y - hz),
        Vec3::new(roof.centre.x + hx, y, roof.centre.y + hz),
        Vec3::new(roof.centre.x - hx, y, roof.centre.y + hz),
    ];
    let pitch = roof.pitch_degrees.to_radians();
    match roof.kind {
        RoofKind::Gable => match roof.ridge_axis {
            RidgeAxis::Z => {
                let rise = hx * pitch.tan();
                let a = Vec3::new(roof.centre.x, y + rise, roof.centre.y - hz);
                let b = Vec3::new(roof.centre.x, y + rise, roof.centre.y + hz);
                vec![
                    vec![corners[0], corners[3], b, a],
                    vec![corners[2], corners[1], a, b],
                ]
            }
            RidgeAxis::X => {
                let rise = hz * pitch.tan();
                let a = Vec3::new(roof.centre.x - hx, y + rise, roof.centre.y);
                let b = Vec3::new(roof.centre.x + hx, y + rise, roof.centre.y);
                vec![
                    vec![corners[1], corners[0], a, b],
                    vec![corners[3], corners[2], b, a],
                ]
            }
        },
        RoofKind::Shed => {
            let rise = match roof.ridge_axis {
                RidgeAxis::Z => hx * 2.0,
                RidgeAxis::X => hz * 2.0,
            } * pitch.tan();
            match roof.ridge_axis {
                RidgeAxis::Z if shed_high_side == Some(Direction::West) => vec![vec![
                    corners[0] + Vec3::Y * rise,
                    corners[3] + Vec3::Y * rise,
                    corners[2],
                    corners[1],
                ]],
                RidgeAxis::Z => vec![vec![
                    corners[0],
                    corners[3],
                    corners[2] + Vec3::Y * rise,
                    corners[1] + Vec3::Y * rise,
                ]],
                RidgeAxis::X if shed_high_side == Some(Direction::South) => vec![vec![
                    corners[0] + Vec3::Y * rise,
                    corners[1] + Vec3::Y * rise,
                    corners[2],
                    corners[3],
                ]],
                RidgeAxis::X => vec![vec![
                    corners[0],
                    corners[1],
                    corners[2] + Vec3::Y * rise,
                    corners[3] + Vec3::Y * rise,
                ]],
            }
        }
        RoofKind::Flat => vec![corners.to_vec()],
        RoofKind::HalfHip => {
            // Project half-hip profile: the upper 45% of each gable is folded
            // back as a short hip while the lower gable remains vertical.
            // This is topology, not a label or a shortened full-hip ridge.
            let shoulder_fraction = 0.55;
            match roof.ridge_axis {
                RidgeAxis::Z => {
                    let rise = hx * pitch.tan();
                    let shoulder_x = hx * (1.0 - shoulder_fraction);
                    let shoulder_y = y + rise * shoulder_fraction;
                    let ridge_half = (hz - hx * 0.45).max(0.0);
                    let south_ridge =
                        Vec3::new(roof.centre.x, y + rise, roof.centre.y - ridge_half);
                    let north_ridge =
                        Vec3::new(roof.centre.x, y + rise, roof.centre.y + ridge_half);
                    let south_w =
                        Vec3::new(roof.centre.x - shoulder_x, shoulder_y, roof.centre.y - hz);
                    let south_e =
                        Vec3::new(roof.centre.x + shoulder_x, shoulder_y, roof.centre.y - hz);
                    let north_w =
                        Vec3::new(roof.centre.x - shoulder_x, shoulder_y, roof.centre.y + hz);
                    let north_e =
                        Vec3::new(roof.centre.x + shoulder_x, shoulder_y, roof.centre.y + hz);
                    vec![
                        vec![
                            corners[0],
                            corners[3],
                            north_w,
                            north_ridge,
                            south_ridge,
                            south_w,
                        ],
                        vec![
                            corners[2],
                            corners[1],
                            south_e,
                            south_ridge,
                            north_ridge,
                            north_e,
                        ],
                        vec![south_w, south_ridge, south_e],
                        vec![north_e, north_ridge, north_w],
                    ]
                }
                RidgeAxis::X => {
                    let rise = hz * pitch.tan();
                    let shoulder_z = hz * (1.0 - shoulder_fraction);
                    let shoulder_y = y + rise * shoulder_fraction;
                    let ridge_half = (hx - hz * 0.45).max(0.0);
                    let west_ridge = Vec3::new(roof.centre.x - ridge_half, y + rise, roof.centre.y);
                    let east_ridge = Vec3::new(roof.centre.x + ridge_half, y + rise, roof.centre.y);
                    let west_s =
                        Vec3::new(roof.centre.x - hx, shoulder_y, roof.centre.y - shoulder_z);
                    let west_n =
                        Vec3::new(roof.centre.x - hx, shoulder_y, roof.centre.y + shoulder_z);
                    let east_s =
                        Vec3::new(roof.centre.x + hx, shoulder_y, roof.centre.y - shoulder_z);
                    let east_n =
                        Vec3::new(roof.centre.x + hx, shoulder_y, roof.centre.y + shoulder_z);
                    vec![
                        vec![
                            corners[1], corners[0], west_s, west_ridge, east_ridge, east_s,
                        ],
                        vec![
                            corners[3], corners[2], east_n, east_ridge, west_ridge, west_n,
                        ],
                        vec![west_n, west_ridge, west_s],
                        vec![east_s, east_ridge, east_n],
                    ]
                }
            }
        }
        RoofKind::Hip | RoofKind::Pavilion => {
            let (ridge_half, rise) = match roof.ridge_axis {
                RidgeAxis::Z => {
                    let inset = if roof.kind == RoofKind::Pavilion {
                        hz
                    } else {
                        hx.min(hz * 0.85)
                    };
                    ((hz - inset).max(0.0), hx * pitch.tan())
                }
                RidgeAxis::X => {
                    let inset = if roof.kind == RoofKind::Pavilion {
                        hx
                    } else {
                        hz.min(hx * 0.85)
                    };
                    ((hx - inset).max(0.0), hz * pitch.tan())
                }
            };
            if roof.kind == RoofKind::Pavilion {
                let apex = Vec3::new(roof.centre.x, y + rise, roof.centre.y);
                vec![
                    vec![corners[0], corners[3], apex],
                    vec![corners[2], corners[1], apex],
                    vec![corners[1], corners[0], apex],
                    vec![corners[3], corners[2], apex],
                ]
            } else {
                match roof.ridge_axis {
                    RidgeAxis::Z => {
                        let a = Vec3::new(roof.centre.x, y + rise, roof.centre.y - ridge_half);
                        let b = Vec3::new(roof.centre.x, y + rise, roof.centre.y + ridge_half);
                        vec![
                            vec![corners[0], corners[3], b, a],
                            vec![corners[2], corners[1], a, b],
                            vec![corners[1], corners[0], a],
                            vec![corners[3], corners[2], b],
                        ]
                    }
                    RidgeAxis::X => {
                        let a = Vec3::new(roof.centre.x - ridge_half, y + rise, roof.centre.y);
                        let b = Vec3::new(roof.centre.x + ridge_half, y + rise, roof.centre.y);
                        vec![
                            vec![corners[1], corners[0], a, b],
                            vec![corners[3], corners[2], b, a],
                            vec![corners[0], corners[3], a],
                            vec![corners[2], corners[1], b],
                        ]
                    }
                }
            }
        }
        RoofKind::Conical => {
            let radius = hx.max(hz);
            let apex = Vec3::new(roof.centre.x, y + radius * pitch.tan(), roof.centre.y);
            (0..24)
                .map(|index| {
                    let a = std::f32::consts::TAU * index as f32 / 24.0;
                    let b = std::f32::consts::TAU * (index + 1) as f32 / 24.0;
                    vec![
                        Vec3::new(
                            roof.centre.x + a.cos() * radius,
                            y,
                            roof.centre.y + a.sin() * radius,
                        ),
                        Vec3::new(
                            roof.centre.x + b.cos() * radius,
                            y,
                            roof.centre.y + b.sin() * radius,
                        ),
                        apex,
                    ]
                })
                .collect()
        }
    }
}

fn same_roof_vertex(left: Vec3, right: Vec3) -> bool {
    (left - right).length_squared() <= 0.000_004
}

fn clip_plan_polygon_to_rect(mut polygon: Vec<Vec2>, min: Vec2, max: Vec2) -> Vec<Vec2> {
    for (axis, value, keep_greater) in [
        (0_usize, min.x, true),
        (0, max.x, false),
        (1, min.y, true),
        (1, max.y, false),
    ] {
        if polygon.is_empty() {
            break;
        }
        let input = std::mem::take(&mut polygon);
        let coordinate = |point: Vec2| if axis == 0 { point.x } else { point.y };
        let inside = |point: Vec2| {
            if keep_greater {
                coordinate(point) >= value - 0.0001
            } else {
                coordinate(point) <= value + 0.0001
            }
        };
        for index in 0..input.len() {
            let current = input[index];
            let previous = input[(index + input.len() - 1) % input.len()];
            let current_inside = inside(current);
            let previous_inside = inside(previous);
            if current_inside != previous_inside {
                let denominator = coordinate(current) - coordinate(previous);
                let fraction = if denominator.abs() <= 0.000_001 {
                    0.0
                } else {
                    (value - coordinate(previous)) / denominator
                };
                polygon.push(previous.lerp(current, fraction));
            }
            if current_inside {
                polygon.push(current);
            }
        }
    }
    polygon
}

fn signed_plan_area(polygon: &[Vec2]) -> f32 {
    polygon
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let next = polygon[(index + 1) % polygon.len()];
            point.x * next.y - next.x * point.y
        })
        .sum::<f32>()
        * 0.5
}

fn convex_plan_hull(mut points: Vec<Vec2>) -> Vec<Vec2> {
    points.sort_by(|left, right| {
        left.x
            .total_cmp(&right.x)
            .then_with(|| left.y.total_cmp(&right.y))
    });
    points.dedup_by(|left, right| left.distance_squared(*right) <= 0.000_004);
    if points.len() < 3 {
        return points;
    }
    let cross = |origin: Vec2, a: Vec2, b: Vec2| (a - origin).perp_dot(b - origin);
    let mut lower = Vec::new();
    for point in points.iter().copied() {
        while lower.len() >= 2
            && cross(lower[lower.len() - 2], lower[lower.len() - 1], point) <= 0.000_002
        {
            lower.pop();
        }
        lower.push(point);
    }
    let mut upper = Vec::new();
    for point in points.iter().rev().copied() {
        while upper.len() >= 2
            && cross(upper[upper.len() - 2], upper[upper.len() - 1], point) <= 0.000_002
        {
            upper.pop();
        }
        upper.push(point);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

fn clip_plan_polygon_to_convex(mut polygon: Vec<Vec2>, clip: &[Vec2]) -> Vec<Vec2> {
    if clip.len() < 3 {
        return Vec::new();
    }
    let orientation = signed_plan_area(clip).signum();
    for edge_index in 0..clip.len() {
        if polygon.is_empty() {
            break;
        }
        let edge_start = clip[edge_index];
        let edge_end = clip[(edge_index + 1) % clip.len()];
        let edge = edge_end - edge_start;
        let side = |point: Vec2| orientation * edge.perp_dot(point - edge_start);
        let input = std::mem::take(&mut polygon);
        for index in 0..input.len() {
            let current = input[index];
            let previous = input[(index + input.len() - 1) % input.len()];
            let current_side = side(current);
            let previous_side = side(previous);
            let current_inside = current_side >= -0.0001;
            let previous_inside = previous_side >= -0.0001;
            if current_inside != previous_inside {
                let denominator = previous_side - current_side;
                let fraction = if denominator.abs() <= 0.000_001 {
                    0.0
                } else {
                    previous_side / denominator
                };
                polygon.push(previous.lerp(current, fraction));
            }
            if current_inside {
                polygon.push(current);
            }
        }
    }
    polygon
}

fn roof_plane_height(plane: RoofPlaneEquation, point: Vec2) -> f32 {
    -(plane.normal.x * point.x + plane.normal.z * point.y + plane.constant) / plane.normal.y
}

fn plan_point_in_convex_polygon(point: Vec2, polygon: &[Vec2]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let orientation = signed_plan_area(polygon).signum();
    polygon.iter().enumerate().all(|(index, start)| {
        let end = polygon[(index + 1) % polygon.len()];
        orientation * (end - *start).perp_dot(point - *start) >= -0.002
    })
}

fn plan_point_in_polygon(point: Vec2, polygon: &[Vec2]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    for index in 0..polygon.len() {
        let start = polygon[index];
        let end = polygon[(index + 1) % polygon.len()];
        let crosses = (start.y > point.y) != (end.y > point.y)
            && point.x
                < (end.x - start.x) * (point.y - start.y) / (end.y - start.y).abs().max(0.000_001)
                    * (end.y - start.y).signum()
                    + start.x;
        if crosses {
            inside = !inside;
        }
    }
    inside
}

fn roof_surface_height_at(assembly: &RoofAssembly, point: Vec2) -> Option<f32> {
    assembly.faces.iter().find_map(|face| {
        let projected = face
            .polygon
            .iter()
            .map(|vertex| Vec2::new(vertex.x, vertex.z))
            .collect::<Vec<_>>();
        plan_point_in_convex_polygon(point, &projected)
            .then(|| roof_plane_height(face.plane, point))
    })
}

fn roof_underside_height_at(assembly: &RoofAssembly, point: Vec2) -> Option<f32> {
    assembly.faces.iter().find_map(|face| {
        let projected = face
            .polygon
            .iter()
            .map(|vertex| Vec2::new(vertex.x, vertex.z))
            .collect::<Vec<_>>();
        plan_point_in_convex_polygon(point, &projected).then(|| {
            roof_plane_height(face.plane, point)
                - face.plane.normal.normalize_or_zero().y * face.thickness_metres
        })
    })
}

fn ray_segment_intersection(origin: Vec2, direction: Vec2, a: Vec2, b: Vec2) -> Option<Vec2> {
    let edge = b - a;
    let denominator = direction.perp_dot(edge);
    if denominator.abs() <= 0.000_001 {
        return None;
    }
    let offset = a - origin;
    let ray_t = offset.perp_dot(edge) / denominator;
    let edge_t = offset.perp_dot(direction) / denominator;
    (ray_t >= -0.002 && (-0.002..=1.002).contains(&edge_t))
        .then(|| origin + direction * ray_t.max(0.0))
}

fn finalize_roof_drainage(
    archetype: BuildingArchetype,
    assemblies: &mut [RoofAssembly],
    geometry: &mut ResolvedGeometry,
) {
    let owners = assemblies
        .iter()
        .map(|roof| roof.owner)
        .collect::<HashSet<_>>();
    geometry
        .roof_drainage_networks
        .retain(|network| !owners.contains(&network.owner));
    geometry
        .solids
        .retain(|solid| !(owners.contains(&solid.owner) && solid.role == SolidRole::RoofGutter));

    for assembly in assemblies {
        for (face_index, face) in assembly.faces.iter().enumerate() {
            let Some(edge) = assembly
                .edges
                .iter()
                .filter(|edge| {
                    edge.adjacent_faces.contains(&face.id)
                        && matches!(edge.kind, RoofEdgeKind::Eave | RoofEdgeKind::Valley)
                })
                .min_by(|left, right| {
                    ((left.start.y + left.end.y) * 0.5)
                        .total_cmp(&((right.start.y + right.end.y) * 0.5))
                })
            else {
                continue;
            };
            let edge_a = Vec2::new(edge.start.x, edge.start.z);
            let edge_b = Vec2::new(edge.end.x, edge.end.z);
            let edge_delta = edge_b - edge_a;
            let edge_length = edge_delta.length().max(0.05);
            let tangent = edge_delta / edge_length;
            let centre = face.polygon.iter().copied().sum::<Vec3>() / face.polygon.len() as f32;
            let downhill = Vec2::new(
                face.plane.normal.x / face.plane.normal.y,
                face.plane.normal.z / face.plane.normal.y,
            )
            .normalize_or_zero();
            let projected = face
                .polygon
                .iter()
                .map(|point| Vec2::new(point.x, point.z))
                .collect::<Vec<_>>();
            let plan_min = projected
                .iter()
                .copied()
                .fold(Vec2::splat(f32::INFINITY), Vec2::min);
            let plan_max = projected
                .iter()
                .copied()
                .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
            let cutouts = face
                .cutouts
                .iter()
                .map(|cutout| {
                    cutout
                        .iter()
                        .map(|point| Vec2::new(point.x, point.z))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            let mut sample_origins = Vec::new();
            for x_step in 0..5 {
                for z_step in 0..5 {
                    let fraction =
                        Vec2::new((x_step as f32 + 0.5) / 5.0, (z_step as f32 + 0.5) / 5.0);
                    let point = plan_min + (plan_max - plan_min) * fraction;
                    if plan_point_in_convex_polygon(point, &projected)
                        && !cutouts
                            .iter()
                            .any(|cutout| plan_point_in_convex_polygon(point, cutout))
                    {
                        sample_origins.push(point);
                    }
                }
            }
            let samples = sample_origins
                .into_iter()
                .filter_map(|origin| {
                    let hit = ray_segment_intersection(origin, downhill, edge_a, edge_b)?;
                    let surface_y = roof_plane_height(face.plane, origin);
                    let edge_y = roof_plane_height(face.plane, hit);
                    (surface_y > edge_y + 0.005).then_some(RoofDrainageSample {
                        surface_point: Vec3::new(origin.x, surface_y, origin.y),
                        channel_inlet: Vec3::new(hit.x, edge_y - 0.025, hit.y),
                    })
                })
                .collect::<Vec<_>>();

            let serial = face_index as u64 * 8;
            let base = (0x8_u64 << 60) | (assembly.id.0 << 16) | 0x6000 | serial;
            let floor_id = ResolvedItemId(base);
            let lip_ids = [ResolvedItemId(base | 1), ResolvedItemId(base | 2)];
            let downspout_id = ResolvedItemId(base | 3);
            let network_id = ResolvedItemId(
                (0x7_u64 << 60) | (assembly.id.0 << 16) | 0x6000 | face_index as u64,
            );
            let compact_child_eave = assembly.parent.is_some()
                && matches!(assembly.kind, RoofKind::Gable | RoofKind::Shed);
            let gutter_width = if compact_child_eave { 0.085 } else { 0.18 };
            let gutter_floor_thickness = if compact_child_eave { 0.018 } else { 0.035 };
            let gutter_lip_height = if compact_child_eave { 0.040 } else { 0.11 };
            let gutter_lip_thickness = if compact_child_eave { 0.018 } else { 0.035 };
            let drop = if compact_child_eave {
                (edge_length * 0.006).max(0.018)
            } else {
                (edge_length * 0.012).max(0.045)
            };
            let lexical_forward = (edge_b.x, edge_b.y) >= (edge_a.x, edge_a.y);
            let (high_plan, low_plan, channel_tangent) = if lexical_forward {
                (edge_a, edge_b, tangent)
            } else {
                (edge_b, edge_a, -tangent)
            };
            let edge_mean_y = (edge.start.y + edge.end.y) * 0.5;
            let mut high = Vec3::new(
                high_plan.x,
                edge_mean_y - gutter_floor_thickness,
                high_plan.y,
            );
            let mut low = Vec3::new(
                low_plan.x,
                edge_mean_y - gutter_floor_thickness - drop,
                low_plan.y,
            );
            let outward = (Vec2::new((high.x + low.x) * 0.5, (high.z + low.z) * 0.5)
                - Vec2::new(centre.x, centre.z))
            .normalize_or_zero();
            if assembly.parent.is_none()
                && matches!(
                    archetype,
                    BuildingArchetype::CastleGatehouse | BuildingArchetype::CourtyardCastle
                )
            {
                // Defensive roof edges sit over deep masonry returns. Keep
                // the physical gutter outside that face instead of centring
                // it inside the wall and escaping with a diagonal collector.
                let fascia_offset = Vec3::new(outward.x, 0.0, outward.y) * 0.35;
                high += fascia_offset;
                low += fascia_offset;
            }
            let channel_centre = (high + low) * 0.5;
            let yaw = channel_tangent.y.atan2(channel_tangent.x);
            let longfall = -drop.atan2(edge_length);
            geometry.solids.push(ResolvedSolid {
                id: floor_id,
                owner: assembly.owner,
                centre: channel_centre,
                size: Vec3::new(edge_length, gutter_floor_thickness, gutter_width),
                yaw_radians: yaw,
                crossfall_radians: 0.0,
                longfall_radians: longfall,
                role: SolidRole::RoofGutter,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: face.support_nodes.clone(),
            });
            geometry.support_interfaces.push(SupportInterface {
                id: ResolvedItemId((0x9_u64 << 60) | (assembly.id.0 << 16) | 0x6000 | serial),
                owner: assembly.owner,
                node: face.support_nodes[0],
                bounds: ResolvedBounds {
                    min: channel_centre - Vec3::splat(0.035),
                    max: channel_centre + Vec3::splat(0.035),
                },
            });
            let lip_offset = Vec3::new(outward.x, 0.0, outward.y) * (gutter_width * 0.5 - 0.008);
            for (lip_slot, (lip_id, sign)) in lip_ids.into_iter().zip([-1.0_f32, 1.0]).enumerate() {
                let lip_centre = channel_centre
                    + lip_offset * sign
                    + Vec3::Y * (gutter_lip_height * 0.5 - gutter_floor_thickness * 0.25);
                geometry.solids.push(ResolvedSolid {
                    id: lip_id,
                    owner: assembly.owner,
                    centre: lip_centre,
                    size: Vec3::new(edge_length, gutter_lip_height, gutter_lip_thickness),
                    yaw_radians: yaw,
                    crossfall_radians: 0.0,
                    longfall_radians: longfall,
                    role: SolidRole::RoofGutter,
                    shape: crate::ResolvedSolidShape::Cuboid,
                    supported_by: face.support_nodes.clone(),
                });
                geometry.support_interfaces.push(SupportInterface {
                    id: ResolvedItemId(
                        (0x9_u64 << 60)
                            | (assembly.id.0 << 16)
                            | 0x6000
                            | serial
                            | (lip_slot as u64 + 1),
                    ),
                    owner: assembly.owner,
                    node: face.support_nodes[0],
                    bounds: ResolvedBounds {
                        min: lip_centre - Vec3::splat(0.035),
                        max: lip_centre + Vec3::splat(0.035),
                    },
                });
            }
            let outlet_plan = low_plan
                + channel_tangent * if compact_child_eave { 0.03 } else { 0.08 }
                + outward * if compact_child_eave { 0.09 } else { 0.38 };
            let outlet = Vec3::new(outlet_plan.x, low.y - 0.025, outlet_plan.y);
            let discharge = Vec3::new(outlet.x, 0.24, outlet.z);
            let outlet_id = geometry
                .drainage_catchments
                .iter()
                .find(|catchment| catchment.id == face.drainage_catchment)
                .and_then(|catchment| {
                    geometry
                        .drainage_routes
                        .iter()
                        .find(|route| route.id == catchment.outlet_route)
                        .map(|route| route.outlet_void)
                })
                .expect("roof face drainage outlet");
            if let Some(void) = geometry.voids.iter_mut().find(|void| void.id == outlet_id) {
                void.bounds = ResolvedBounds {
                    min: outlet - Vec3::splat(0.04),
                    max: outlet + Vec3::splat(0.04),
                };
            }
            if let Some(catchment) = geometry
                .drainage_catchments
                .iter_mut()
                .find(|catchment| catchment.id == face.drainage_catchment)
            {
                catchment.toe_channel_solids = vec![floor_id, lip_ids[0], lip_ids[1]];
                catchment.tangent = channel_tangent;
                catchment.outward = outward;
                catchment.outlet_along_metres = edge_length * 0.5;
            }
            if let Some(route) = geometry
                .drainage_routes
                .iter_mut()
                .find(|route| route.outlet_void == outlet_id)
            {
                route.inlet = samples.first().map_or(high, |sample| sample.surface_point);
                route.outlet = outlet;
            }
            geometry.solids.push(ResolvedSolid {
                id: downspout_id,
                owner: assembly.owner,
                centre: (outlet + discharge) * 0.5 - Vec3::Y * 0.10,
                size: Vec3::new(0.09, (outlet.y - discharge.y - 0.20).max(0.09), 0.09),
                yaw_radians: 0.0,
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::RoofGutter,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: face.support_nodes.clone(),
            });
            let spout_top = Vec3::new(outlet.x, outlet.y - 0.20, outlet.z);
            geometry.support_interfaces.push(SupportInterface {
                id: ResolvedItemId((0x9_u64 << 60) | (assembly.id.0 << 16) | 0x6000 | serial | 3),
                owner: assembly.owner,
                node: face.support_nodes[0],
                bounds: ResolvedBounds {
                    min: spout_top - Vec3::splat(0.035),
                    max: spout_top + Vec3::splat(0.035),
                },
            });
            geometry.roof_drainage_networks.push(RoofDrainageNetwork {
                id: network_id,
                owner: assembly.owner,
                face: face.id,
                catchment: face.drainage_catchment,
                receiving_edge: edge.id,
                samples,
                channel_floor: floor_id,
                channel_lips: lip_ids,
                collector_solids: Vec::new(),
                outlet_station: network_id,
                outlet_void: outlet_id,
                downspout: Some(downspout_id),
                channel_high: high,
                channel_low: low,
                discharge,
            });
        }
    }
}

fn supplement_split_eave_drainage(assemblies: &[RoofAssembly], geometry: &mut ResolvedGeometry) {
    for assembly in assemblies {
        for link in assembly.children.iter().filter(|link| {
            link.kind == RoofChildKind::CrossGable && link.split_eave_edges.len() == 3
        }) {
            // The two retained eaves and the recessed apron at the facade cut are
            // distinct physical recipients.  The apron is not an eave relabel: it
            // catches the narrow strip of parent weather face that terminates at
            // the Zwerchhaus opening instead of allowing it to discharge onto the
            // facade or through the opening cut.
            let receivers = [
                link.split_eave_edges[0],
                link.split_eave_edges[1],
                link.split_eave_edges[2],
            ];
            let existing_edges = geometry
                .roof_drainage_networks
                .iter()
                .filter(|network| network.owner == assembly.owner)
                .map(|network| network.receiving_edge)
                .collect::<HashSet<_>>();
            for (slot, edge_id) in receivers.into_iter().enumerate() {
                if existing_edges.contains(&edge_id) {
                    continue;
                }
                let Some(edge) = assembly.edges.iter().find(|edge| edge.id == edge_id) else {
                    continue;
                };
                let Some(face) = assembly
                    .faces
                    .iter()
                    .find(|face| edge.adjacent_faces.contains(&face.id))
                else {
                    continue;
                };
                let a = Vec2::new(edge.start.x, edge.start.z);
                let b = Vec2::new(edge.end.x, edge.end.z);
                let delta = b - a;
                let length = delta.length().max(0.05);
                let tangent = delta / length;
                let centre = face.polygon.iter().copied().sum::<Vec3>() / face.polygon.len() as f32;
                let downhill = Vec2::new(
                    face.plane.normal.x / face.plane.normal.y,
                    face.plane.normal.z / face.plane.normal.y,
                )
                .normalize_or_zero();
                let projected = face
                    .polygon
                    .iter()
                    .map(|point| Vec2::new(point.x, point.z))
                    .collect::<Vec<_>>();
                let min = projected
                    .iter()
                    .copied()
                    .fold(Vec2::splat(f32::INFINITY), Vec2::min);
                let max = projected
                    .iter()
                    .copied()
                    .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
                let cutouts = face
                    .cutouts
                    .iter()
                    .map(|cut| cut.iter().map(|p| Vec2::new(p.x, p.z)).collect::<Vec<_>>())
                    .collect::<Vec<_>>();
                let mut samples = Vec::new();
                for x in 0..5 {
                    for z in 0..5 {
                        let fraction = Vec2::new((x as f32 + 0.5) / 5.0, (z as f32 + 0.5) / 5.0);
                        let origin = min + (max - min) * fraction;
                        if !plan_point_in_convex_polygon(origin, &projected)
                            || cutouts
                                .iter()
                                .any(|cut| plan_point_in_convex_polygon(origin, cut))
                        {
                            continue;
                        }
                        let Some(hit) = ray_segment_intersection(origin, downhill, a, b) else {
                            continue;
                        };
                        let surface_y = roof_plane_height(face.plane, origin);
                        let edge_y = roof_plane_height(face.plane, hit);
                        if surface_y > edge_y + 0.005 {
                            samples.push(RoofDrainageSample {
                                surface_point: Vec3::new(origin.x, surface_y, origin.y),
                                channel_inlet: Vec3::new(hit.x, edge_y - 0.025, hit.y),
                            });
                        }
                    }
                }
                if samples.is_empty() {
                    continue;
                }
                let serial = 0x6800 | ((link.child.0 & 0xFF) << 4) | (slot as u64 * 4);
                let floor = ResolvedItemId((0x8_u64 << 60) | (assembly.id.0 << 16) | serial);
                let lips = [ResolvedItemId(floor.0 | 1), ResolvedItemId(floor.0 | 2)];
                let spout = ResolvedItemId(floor.0 | 3);
                let catchment = ResolvedItemId((0xC_u64 << 60) | (assembly.id.0 << 16) | serial);
                let route = ResolvedItemId((0xD_u64 << 60) | (assembly.id.0 << 16) | serial);
                let outlet_void = ResolvedItemId((0xE_u64 << 60) | (assembly.id.0 << 16) | serial);
                let network = ResolvedItemId((0x7_u64 << 60) | (assembly.id.0 << 16) | serial);
                let forward = (b.x, b.y) >= (a.x, a.y);
                let (high_plan, low_plan, channel_tangent) = if forward {
                    (a, b, tangent)
                } else {
                    (b, a, -tangent)
                };
                let drop = (length * 0.012).max(0.045);
                let mean_y = (edge.start.y + edge.end.y) * 0.5;
                let high = Vec3::new(high_plan.x, mean_y - 0.035, high_plan.y);
                let low = Vec3::new(low_plan.x, mean_y - 0.035 - drop, low_plan.y);
                let outward = (Vec2::new((high.x + low.x) * 0.5, (high.z + low.z) * 0.5)
                    - Vec2::new(centre.x, centre.z))
                .normalize_or_zero();
                let channel_centre = (high + low) * 0.5;
                let yaw = channel_tangent.y.atan2(channel_tangent.x);
                let longfall = -drop.atan2(length);
                let lip_offset = Vec3::new(outward.x, 0.0, outward.y) * 0.075;
                for (id, item_centre, size) in [
                    (floor, channel_centre, Vec3::new(length, 0.035, 0.18)),
                    (
                        lips[0],
                        channel_centre - lip_offset + Vec3::Y * 0.045,
                        Vec3::new(length, 0.11, 0.035),
                    ),
                    (
                        lips[1],
                        channel_centre + lip_offset + Vec3::Y * 0.045,
                        Vec3::new(length, 0.11, 0.035),
                    ),
                ] {
                    geometry.solids.push(ResolvedSolid {
                        id,
                        owner: assembly.owner,
                        centre: item_centre,
                        size,
                        yaw_radians: yaw,
                        crossfall_radians: 0.0,
                        longfall_radians: longfall,
                        role: SolidRole::RoofGutter,
                        shape: crate::ResolvedSolidShape::Cuboid,
                        supported_by: face.support_nodes.clone(),
                    });
                    geometry.support_interfaces.push(SupportInterface {
                        id: ResolvedItemId((0x9_u64 << 60) | (id.0 & 0x0FFF_FFFF_FFFF_FFFF)),
                        owner: assembly.owner,
                        node: face.support_nodes[0],
                        bounds: ResolvedBounds {
                            min: item_centre - Vec3::splat(0.035),
                            max: item_centre + Vec3::splat(0.035),
                        },
                    });
                }
                let outlet_plan = low_plan + channel_tangent * 0.08 + outward * 0.38;
                let outlet = Vec3::new(outlet_plan.x, low.y - 0.025, outlet_plan.y);
                let discharge = Vec3::new(outlet.x, 0.24, outlet.z);
                geometry.voids.push(ResolvedVoid {
                    id: outlet_void,
                    owner: assembly.owner,
                    bounds: ResolvedBounds {
                        min: outlet - Vec3::splat(0.04),
                        max: outlet + Vec3::splat(0.04),
                    },
                    role: VoidRole::Drain,
                    shape: crate::ResolvedVoidShape::Box,
                    subtracts_from: assembly.owner,
                });
                geometry.drainage_routes.push(DrainageRoute {
                    id: route,
                    owner: assembly.owner,
                    outlet_void,
                    inlet: samples[0].surface_point,
                    outlet,
                });
                geometry.surfaces.push(ResolvedSurface {
                    id: catchment,
                    owner: assembly.owner,
                    bounds: roof_polygon_bounds(&face.polygon),
                    role: SurfaceRole::RoofDrainage,
                    shape: crate::ResolvedSurfaceShape::Planar,
                });
                geometry.drainage_catchments.push(DrainageCatchment {
                    id: catchment,
                    owner: assembly.owner,
                    walk_solid: face.id,
                    toe_channel_solids: vec![floor, lips[0], lips[1]],
                    drainage_surface: catchment,
                    outlet_route: route,
                    centre,
                    tangent: channel_tangent,
                    outward,
                    length_metres: length,
                    width_metres: 0.18,
                    inner_elevation_metres: samples[0].surface_point.y,
                    outer_elevation_metres: low.y,
                    outlet_along_metres: length * 0.5,
                });
                geometry.solids.push(ResolvedSolid {
                    id: spout,
                    owner: assembly.owner,
                    centre: (outlet + discharge) * 0.5 - Vec3::Y * 0.10,
                    size: Vec3::new(0.09, (outlet.y - discharge.y - 0.20).max(0.09), 0.09),
                    yaw_radians: 0.0,
                    crossfall_radians: 0.0,
                    longfall_radians: 0.0,
                    role: SolidRole::RoofGutter,
                    shape: crate::ResolvedSolidShape::Cuboid,
                    supported_by: face.support_nodes.clone(),
                });
                let spout_top = Vec3::new(outlet.x, outlet.y - 0.20, outlet.z);
                geometry.support_interfaces.push(SupportInterface {
                    id: ResolvedItemId((0x9_u64 << 60) | (spout.0 & 0x0FFF_FFFF_FFFF_FFFF)),
                    owner: assembly.owner,
                    node: face.support_nodes[0],
                    bounds: ResolvedBounds {
                        min: spout_top - Vec3::splat(0.035),
                        max: spout_top + Vec3::splat(0.035),
                    },
                });
                geometry.roof_drainage_networks.push(RoofDrainageNetwork {
                    id: network,
                    owner: assembly.owner,
                    face: face.id,
                    catchment,
                    receiving_edge: edge.id,
                    samples,
                    channel_floor: floor,
                    channel_lips: lips,
                    collector_solids: Vec::new(),
                    outlet_station: network,
                    outlet_void,
                    downspout: Some(spout),
                    channel_high: high,
                    channel_low: low,
                    discharge,
                });
            }
        }
    }
}

/// Consolidate face gutters into a small set of physical outlet stations.
/// Project gates: principal gable/hip roofs use at most two stations and
/// round/pavilion roofs at most four. Attached children explicitly free-drip
/// to the parent weather face instead of growing a detached pipe to grade.
fn consolidate_roof_outlet_stations(
    archetype: BuildingArchetype,
    assemblies: &mut [RoofAssembly],
    stairs: &[Stair],
    walls: &[crate::WallAssembly],
    _openings: &[crate::OpeningAssembly],
    geometry: &mut ResolvedGeometry,
) {
    geometry.roof_drainage_outlets.clear();
    let assembly_read = assemblies.to_vec();
    for assembly in &assembly_read {
        let cross_facade_wall = assembly.parent.and_then(|parent_id| {
            assembly_read
                .iter()
                .find(|parent| parent.id == parent_id)
                .and_then(|parent| {
                    parent.children.iter().find_map(|child| {
                        (child.child == assembly.id && child.kind == RoofChildKind::CrossGable)
                            .then_some(child.facade_wall)
                            .flatten()
                    })
                })
        });
        let is_cross_gable = assembly.parent.is_some_and(|parent_id| {
            assembly_read.iter().any(|parent| {
                parent.id == parent_id
                    && parent.children.iter().any(|child| {
                        child.child == assembly.id && child.kind == RoofChildKind::CrossGable
                    })
            })
        });
        let is_timber_child = assembly.parent.is_some()
            && matches!(
                archetype,
                BuildingArchetype::TownHouse
                    | BuildingArchetype::HallHouse
                    | BuildingArchetype::FachwerkCottage
                    | BuildingArchetype::FachwerkMerchantHouse
                    | BuildingArchetype::RenaissanceTownHall
            );
        let mut network_indices = geometry
            .roof_drainage_networks
            .iter()
            .enumerate()
            .filter_map(|(index, network)| (network.owner == assembly.owner).then_some(index))
            .collect::<Vec<_>>();
        if network_indices.is_empty() {
            continue;
        }
        let roof_centre = assembly
            .faces
            .iter()
            .flat_map(|face| &face.polygon)
            .map(|point| Vec2::new(point.x, point.z))
            .sum::<Vec2>()
            / assembly
                .faces
                .iter()
                .map(|face| face.polygon.len())
                .sum::<usize>() as f32;
        network_indices.sort_by(|left, right| {
            let left = geometry.roof_drainage_networks[*left].channel_low;
            let right = geometry.roof_drainage_networks[*right].channel_low;
            (left.z - roof_centre.y)
                .atan2(left.x - roof_centre.x)
                .total_cmp(&(right.z - roof_centre.y).atan2(right.x - roof_centre.x))
        });
        let maximum_stations = match assembly.kind {
            RoofKind::Conical | RoofKind::Pavilion => 4,
            _ => 4,
        };
        let station_count = maximum_stations.min(network_indices.len()).max(1);
        let chunk_size = network_indices.len().div_ceil(station_count);
        for (station_slot, chunk) in network_indices.chunks(chunk_size).enumerate() {
            let mut desired = chunk
                .iter()
                .map(|index| geometry.roof_drainage_networks[*index].channel_low)
                .sum::<Vec3>()
                / chunk.len() as f32;
            if matches!(assembly.kind, RoofKind::Conical | RoofKind::Pavilion) {
                let mean_radius = chunk
                    .iter()
                    .map(|index| {
                        let point = geometry.roof_drainage_networks[*index].channel_low;
                        Vec2::new(point.x, point.z).distance(roof_centre)
                    })
                    .sum::<f32>()
                    / chunk.len() as f32;
                let radial = (Vec2::new(desired.x, desired.z) - roof_centre).normalize_or_zero();
                desired.x = roof_centre.x + radial.x * (mean_radius + 0.14);
                desired.z = roof_centre.y + radial.y * (mean_radius + 0.14);
            } else {
                // Keep ordinary outlets on a real eave endpoint; averaging
                // opposing or stepped eaves manufactures a collector through
                // the roof field.
                let network = &geometry.roof_drainage_networks[chunk[chunk.len() / 2]];
                let endpoint = network.channel_low;
                // Use the receiving eave's exact outward normal. A radial
                // corner vector leaves the drip over the side trimmer on a
                // small dormer, so the later timber pass legitimately blocks
                // it even though the roof-only solver looked clear.
                let outward = geometry
                    .drainage_catchments
                    .iter()
                    .find(|catchment| catchment.id == network.catchment)
                    .map_or_else(
                        || (Vec2::new(endpoint.x, endpoint.z) - roof_centre).normalize_or_zero(),
                        |catchment| catchment.outward,
                    );
                let outlet_offset = if assembly.parent.is_some() {
                    0.10
                } else {
                    0.14
                };
                desired.x = endpoint.x + outward.x * outlet_offset;
                desired.z = endpoint.z + outward.y * outlet_offset;
            }
            let old_outlets = chunk
                .iter()
                .map(|index| geometry.roof_drainage_networks[*index].outlet_void)
                .collect::<HashSet<_>>();
            let old_spouts = chunk
                .iter()
                .filter_map(|index| geometry.roof_drainage_networks[*index].downspout)
                .collect::<HashSet<_>>();
            let shared_outlet = geometry.roof_drainage_networks[chunk[0]].outlet_void;
            let station_id = ResolvedItemId(
                (0x7_u64 << 60) | (assembly.id.0 << 16) | 0x7000 | station_slot as u64,
            );
            let recipient_surface = ResolvedItemId(
                (0x9_u64 << 60) | (assembly.id.0 << 16) | 0x7000 | station_slot as u64,
            );
            let resolved_solids = &geometry.solids;

            let free_drip = assembly.parent.and_then(|parent_id| {
                let parent = assembly_read.iter().find(|roof| roof.id == parent_id)?;
                // A child eave free-drips vertically onto the parent weather
                // face directly below; it does not run an unframed diagonal
                // collector through the dormer enclosure.
                let desired_plan = Vec2::new(desired.x, desired.z);
                let face_contains_recipient = |face: &RoofFace, target_plan: Vec2| {
                    let outline = face
                        .polygon
                        .iter()
                        .map(|point| Vec2::new(point.x, point.z))
                        .collect::<Vec<_>>();
                    plan_point_in_polygon(target_plan, &outline)
                        && !face.cutouts.iter().any(|cutout| {
                            let cutout = cutout
                                .iter()
                                .map(|point| Vec2::new(point.x, point.z))
                                .collect::<Vec<_>>();
                            plan_point_in_polygon(target_plan, &cutout)
                        })
                };
                let ordinary_offsets = [
                    Vec2::ZERO,
                    Vec2::X * 0.25,
                    -Vec2::X * 0.25,
                    Vec2::Y * 0.25,
                    -Vec2::Y * 0.25,
                    Vec2::X * 0.50,
                    -Vec2::X * 0.50,
                    Vec2::Y * 0.50,
                    -Vec2::Y * 0.50,
                    Vec2::X * 0.75,
                    -Vec2::X * 0.75,
                    Vec2::Y * 0.75,
                    -Vec2::Y * 0.75,
                ];
                let tower_offsets = [
                    Vec2::ZERO,
                    Vec2::X * 0.50,
                    -Vec2::X * 0.50,
                    Vec2::Y * 0.50,
                    -Vec2::Y * 0.50,
                    Vec2::X * 0.75,
                    -Vec2::X * 0.75,
                    Vec2::Y * 0.75,
                    -Vec2::Y * 0.75,
                ];
                let offsets: &[Vec2] = if assembly.kind == RoofKind::Pavilion {
                    &tower_offsets
                } else {
                    &ordinary_offsets
                };
                let source_low = chunk
                    .iter()
                    .map(|index| geometry.roof_drainage_networks[*index].channel_low.y)
                    .fold(f32::INFINITY, f32::min);
                let selected = offsets
                    .iter()
                    .copied()
                    .flat_map(|offset| {
                        let target = desired_plan + offset;
                        parent
                            .faces
                            .iter()
                            .filter(move |face| face_contains_recipient(face, target))
                            .filter(move |face| {
                                let recipient_y = roof_plane_height(face.plane, target);
                                resolved_solids.iter().all(|solid| {
                                    if solid.role == SolidRole::RoofFace
                                        || (solid.owner == assembly.owner
                                            && matches!(
                                                solid.role,
                                                SolidRole::RoofGutter
                                                    | SolidRole::RoofEdgeTreatment
                                            ))
                                    {
                                        return true;
                                    }
                                    let cosine = solid.yaw_radians.cos().abs();
                                    let sine = solid.yaw_radians.sin().abs();
                                    let half = Vec3::new(
                                        (solid.size.x * cosine + solid.size.z * sine) * 0.5,
                                        solid.size.y * 0.5,
                                        (solid.size.x * sine + solid.size.z * cosine) * 0.5,
                                    );
                                    let bounds = (solid.centre - half, solid.centre + half);
                                    if solid.role == SolidRole::RoofFlashing
                                        && solid.owner == parent.owner
                                        && bounds.1.y <= recipient_y + 0.86
                                    {
                                        return true;
                                    }
                                    let plan_hit = match solid.shape {
                                        crate::ResolvedSolidShape::RoundTowerShell {
                                            outer_radius_metres,
                                            ..
                                        } => {
                                            target
                                                .distance(Vec2::new(solid.centre.x, solid.centre.z))
                                                <= outer_radius_metres + 0.08
                                        }
                                        _ => {
                                            target.x >= bounds.0.x - 0.08
                                                && target.x <= bounds.1.x + 0.08
                                                && target.y >= bounds.0.z - 0.08
                                                && target.y <= bounds.1.z + 0.08
                                        }
                                    };
                                    let vertical_hit = source_low - 0.15 > bounds.0.y
                                        && recipient_y + 0.14 < bounds.1.y;
                                    !(plan_hit && vertical_hit)
                                })
                            })
                            .map(move |face| (target, face))
                    })
                    .min_by(|left, right| {
                        left.0
                            .distance(desired_plan)
                            .total_cmp(&right.0.distance(desired_plan))
                    });
                if let Some((target_plan, face)) = selected {
                    let recipient_y = roof_plane_height(face.plane, target_plan);
                    if recipient_y + 0.08 < source_low {
                        return Some((parent, face, target_plan, recipient_y));
                    }
                }

                // The obstacle-aware first pass may reject all candidates at
                // a non-convex dormer notch. Select a geometrically exact
                // downhill parent-face point as a fallback; the independent
                // drainage audit then performs the full fall-cone collision
                // sweep and rejects it if anything actually blocks the drop.
                [0.25_f32, 0.50, 0.75, 1.00, 1.25, 1.50]
                    .into_iter()
                    .flat_map(|distance| {
                        [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y]
                            .into_iter()
                            .map(move |axis| desired_plan + axis * distance)
                    })
                    .find_map(|target_plan| {
                        parent.faces.iter().find_map(|face| {
                            if !face_contains_recipient(face, target_plan) {
                                return None;
                            }
                            let recipient_y = roof_plane_height(face.plane, target_plan);
                            (recipient_y + 0.08 < source_low).then_some((
                                parent,
                                face,
                                target_plan,
                                recipient_y,
                            ))
                        })
                    })
            });

            let opening_voids = &geometry.voids;
            let drainage_networks = &geometry.roof_drainage_networks;
            let child_host_candidate = is_timber_child
                .then(|| {
                    let desired_plan = Vec2::new(desired.x, desired.z);
                    let seed = cross_facade_wall
                        .and_then(|facade_wall_id| {
                            walls.iter().find(|wall| wall.id == facade_wall_id)
                        })
                        .or_else(|| {
                            walls
                                .iter()
                                .filter(|wall| {
                                    wall.base_elevation_metres <= 0.30
                                        && wall.frame.outside_room.is_none()
                                        && wall.radial_frame.is_none()
                                })
                                .min_by(|left, right| {
                                    left.frame
                                        .origin
                                        .distance(roof_centre)
                                        .total_cmp(&right.frame.origin.distance(roof_centre))
                                })
                        })?;
                    let away = (desired_plan - roof_centre)
                        .dot(seed.frame.tangent)
                        .signum();
                    let clear_target = desired_plan + seed.frame.tangent * away * 1.20;
                    walls
                        .iter()
                        .filter(|wall| {
                            wall.base_elevation_metres <= 0.30
                                && wall.frame.outward.dot(seed.frame.outward) >= 0.99
                                && (wall.frame.origin - seed.frame.origin)
                                    .dot(seed.frame.outward)
                                    .abs()
                                    <= 0.05
                        })
                        .flat_map(|wall| {
                            let preferred =
                                (clear_target - wall.frame.origin).dot(wall.frame.tangent);
                            [-0.60_f32, -0.30, 0.0, 0.30, 0.60]
                                .into_iter()
                                .map(move |adjustment| {
                                    let along = (preferred + adjustment).clamp(
                                        -wall.length_metres * 0.5 + 0.12,
                                        wall.length_metres * 0.5 - 0.12,
                                    );
                                    let face = wall.frame.origin
                                        + wall.frame.tangent * along
                                        + wall.frame.outward * wall.thickness_metres * 0.5;
                                    (wall, face, face.distance(clear_target))
                                })
                        })
                        .filter(|(_, face, _)| {
                            opening_voids
                                .iter()
                                .filter(|void| {
                                    matches!(
                                        void.role,
                                        VoidRole::WallOpening | VoidRole::AccessPortal
                                    )
                                })
                                .all(|void| {
                                    face.x < void.bounds.min.x - 0.10
                                        || face.x > void.bounds.max.x + 0.10
                                        || face.y < void.bounds.min.z - 0.10
                                        || face.y > void.bounds.max.z + 0.10
                                })
                        })
                        .min_by(|left, right| left.2.total_cmp(&right.2))
                })
                .flatten();
            let host_candidate = if free_drip.is_none() {
                child_host_candidate.or_else(|| {
                    walls
                        .iter()
                        .filter(|wall| {
                            let tower_face =
                                matches!(wall.source, crate::WallSourceId::SquareTowerFace { .. });
                            wall.replaced_by_owner.is_none()
                                && (wall.frame.outside_room.is_none()
                                    || (is_cross_gable
                                        && matches!(
                                            wall.source,
                                            crate::WallSourceId::RoofChildFront { .. }
                                        )))
                                && wall.radial_frame.is_none()
                                && (!matches!(
                                    wall.source,
                                    crate::WallSourceId::RoofChildFront { .. }
                                ) || is_cross_gable)
                                && (tower_face
                                    || wall.base_elevation_metres <= 0.30
                                    || (is_cross_gable
                                        && matches!(
                                            wall.source,
                                            crate::WallSourceId::RoofChildFront { .. }
                                        )))
                        })
                        .flat_map(|wall| {
                            let desired_plan = Vec2::new(desired.x, desired.z);
                            let preferred =
                                (desired_plan - wall.frame.origin).dot(wall.frame.tangent);
                            [-1.20_f32, -0.90, -0.60, -0.30, 0.0, 0.30, 0.60, 0.90, 1.20]
                                .into_iter()
                                .filter_map(move |adjustment| {
                                    let face = if let Some(radial) = wall.radial_frame {
                                        let radius = wall.length_metres / std::f32::consts::TAU;
                                        let desired_axis = (desired_plan - radial.centre)
                                            .normalize_or(radial.reference_outward);
                                        let angle = adjustment / radius.max(0.1);
                                        let cosine = angle.cos();
                                        let sine = angle.sin();
                                        let axis = Vec2::new(
                                            desired_axis.x * cosine - desired_axis.y * sine,
                                            desired_axis.x * sine + desired_axis.y * cosine,
                                        );
                                        radial.centre
                                            + axis * (radius + wall.thickness_metres * 0.5)
                                    } else {
                                        let along = (preferred + adjustment).clamp(
                                            -wall.length_metres * 0.5 + 0.12,
                                            wall.length_metres * 0.5 - 0.12,
                                        );
                                        wall.frame.origin
                                            + wall.frame.tangent * along
                                            + wall.frame.outward * wall.thickness_metres * 0.5
                                    };
                                    // A downspout spans the complete stacked facade, not
                                    // merely this ground-storey wall record. Keep its plan
                                    // station clear of openings on every collinear storey.
                                    let opening_clear = opening_voids
                                        .iter()
                                        .filter(|void| {
                                            matches!(
                                                void.role,
                                                VoidRole::WallOpening | VoidRole::AccessPortal
                                            )
                                        })
                                        .all(|void| {
                                            face.x < void.bounds.min.x - 0.18
                                                || face.x > void.bounds.max.x + 0.18
                                                || face.y < void.bounds.min.z - 0.18
                                                || face.y > void.bounds.max.z + 0.18
                                        });
                                    let collector_start = Vec2::new(
                                        drainage_networks[chunk[0]].channel_low.x,
                                        drainage_networks[chunk[0]].channel_low.z,
                                    );
                                    let collector_clear =
                                        (1..10).all(|sample| {
                                            let point =
                                                collector_start.lerp(face, sample as f32 / 10.0);
                                            resolved_solids.iter().all(|solid| {
                                                if ((!is_timber_child
                                                    && solid.role != SolidRole::WallHost)
                                                    || (is_timber_child
                                                        && !matches!(
                                                            solid.role,
                                                            SolidRole::WallHost
                                                                | SolidRole::OpeningJamb
                                                                | SolidRole::OpeningSill
                                                                | SolidRole::OpeningHead
                                                                | SolidRole::OpeningSpandrel
                                                                | SolidRole::OpeningClosure
                                                        )))
                                                    || wall.host_solids.contains(&solid.id)
                                                    || (is_timber_child
                                                        && solid.centre.y + solid.size.y * 0.5
                                                            < drainage_networks[chunk[0]]
                                                                .channel_low
                                                                .y
                                                                - 0.15)
                                                {
                                                    return true;
                                                }
                                                match solid.shape {
                                            crate::ResolvedSolidShape::RoundTowerShell {
                                                outer_radius_metres,
                                                ..
                                            } => {
                                                point.distance(Vec2::new(
                                                    solid.centre.x,
                                                    solid.centre.z,
                                                )) > outer_radius_metres + 0.06
                                            }
                                            _ => {
                                                let half = solid.size * 0.5;
                                                let margin = if solid.role == SolidRole::WallHost {
                                                    0.06
                                                } else {
                                                    0.30
                                                };
                                                point.x < solid.centre.x - half.x - margin
                                                    || point.x > solid.centre.x + half.x + margin
                                                    || point.y < solid.centre.z - half.z - margin
                                                    || point.y > solid.centre.z + half.z + margin
                                            }
                                        }
                                            })
                                        });
                                    (opening_clear && collector_clear).then_some((
                                        wall,
                                        face,
                                        face.distance(desired_plan),
                                    ))
                                })
                        })
                        .min_by(|left, right| left.2.total_cmp(&right.2))
                })
            } else {
                None
            };

            let (disposition, host_wall, facade_contact, recipient, outlet, discharge, spout_id) =
                if let Some((parent, face, target_plan, recipient_y)) = free_drip {
                    let outlet = Vec3::new(
                        target_plan.x,
                        chunk
                            .iter()
                            .map(|index| geometry.roof_drainage_networks[*index].channel_low.y)
                            .fold(f32::INFINITY, f32::min)
                            - 0.07,
                        target_plan.y,
                    );
                    let discharge = Vec3::new(target_plan.x, recipient_y + 0.06, target_plan.y);
                    (
                        RoofDrainageDisposition::FreeDripToParentRoof,
                        None,
                        None,
                        RoofDrainageRecipient::ParentRoofFace {
                            roof: parent.id,
                            face: face.id,
                        },
                        outlet,
                        discharge,
                        None,
                    )
                } else if let Some((host, face, _distance)) = host_candidate.filter(|candidate| {
                    candidate.2
                        <= if is_cross_gable || is_timber_child {
                            3.20
                        } else {
                            1.20
                        }
                }) {
                    let host_outward = host.radial_frame.map_or(host.frame.outward, |radial| {
                        (face - radial.centre).normalize_or(radial.reference_outward)
                    });
                    let projected_facade_clearance = match archetype {
                        BuildingArchetype::TownHouse => 0.22,
                        BuildingArchetype::FachwerkMerchantHouse => 0.28,
                        BuildingArchetype::RenaissanceTownHall => 0.24,
                        _ => 0.0,
                    };
                    let pipe_plan =
                        face + host_outward * (0.055 + projected_facade_clearance + 0.10);
                    let outlet_y = chunk
                        .iter()
                        .map(|index| geometry.roof_drainage_networks[*index].channel_low.y)
                        .fold(f32::INFINITY, f32::min)
                        - 0.08;
                    let outlet = Vec3::new(pipe_plan.x, outlet_y, pipe_plan.y);
                    let discharge = Vec3::new(pipe_plan.x, 0.24, pipe_plan.y);
                    (
                        RoofDrainageDisposition::BoundDownspout,
                        Some(host.id),
                        Some(Vec3::new(face.x, outlet_y * 0.5, face.y)),
                        RoofDrainageRecipient::GroundSplashApron,
                        outlet,
                        discharge,
                        old_spouts.iter().min().copied(),
                    )
                } else {
                    // Deep overhangs without a facade beneath the eave are an
                    // explicit free-drip condition. Do not manufacture a
                    // detached pipe across open air to the nearest wall.
                    // Move the fall cone a further 200 mm beyond the fascia;
                    // combined with the ordinary outlet offset this freezes a
                    // 340 mm clearance from the eave without displacing child
                    // outlets that must land on a parent weather face.
                    let desired_plan = Vec2::new(desired.x, desired.z);
                    let source_network = &geometry.roof_drainage_networks[chunk[0]];
                    let mut source_channel_ids = vec![source_network.channel_floor];
                    source_channel_ids.extend(source_network.channel_lips);
                    source_channel_ids.extend(source_network.collector_solids.iter().copied());
                    let downhill = assembly
                        .faces
                        .iter()
                        .find(|face| face.id == source_network.face)
                        .map(|face| {
                            Vec2::new(
                                face.plane.normal.x / face.plane.normal.y,
                                face.plane.normal.z / face.plane.normal.y,
                            )
                            .normalize_or_zero()
                        })
                        .unwrap_or_else(|| (desired_plan - roof_centre).normalize_or_zero());
                    let outlet_y = chunk
                        .iter()
                        .map(|index| geometry.roof_drainage_networks[*index].channel_low.y)
                        .fold(f32::INFINITY, f32::min)
                        - 0.07;
                    let channel_low_plan =
                        Vec2::new(source_network.channel_low.x, source_network.channel_low.z);
                    let toward_channel_high =
                        (Vec2::new(source_network.channel_high.x, source_network.channel_high.z)
                            - channel_low_plan)
                            .normalize_or_zero();
                    let defensive_roof = matches!(
                        archetype,
                        BuildingArchetype::CastleGatehouse | BuildingArchetype::CourtyardCastle
                    );
                    let along_offsets: &[f32] = if defensive_roof {
                        &[
                            0.0, 0.60, -0.60, 1.20, -1.20, 1.80, -1.80, 2.40, -2.40, 3.00, -3.00,
                            3.60, -3.60, 4.20, -4.20,
                        ]
                    } else if assembly_read.len() == 1 {
                        &[0.0, 0.30, 0.60, 0.90, 1.20, -0.30, -0.60, -0.90, -1.20]
                    } else {
                        // Try both gutter directions before accepting a free
                        // fall.  Attached pavilion eaves commonly overlap the
                        // choir floor in one direction but can discharge at
                        // the exposed corner in the other.  The ordered
                        // search preserves existing stations whenever their
                        // shorter positive collector remains valid.
                        &[
                            0.0, 0.30, 0.60, -0.30, -0.60, 0.90, -0.90, 1.20, -1.20, 1.50, -1.50,
                            1.80, -1.80,
                        ]
                    };
                    let outward_offsets: &[f32] = if defensive_roof {
                        // A corner catchment first follows its owned eave away
                        // from the return wall.  Only then should it step
                        // outward.  Omitting the zero-offset option forced a
                        // diagonal shortcut through the courtyard corner.
                        &[0.0, 0.20, 0.35, 0.50, 0.65, 0.80, 1.00, 1.20]
                    } else if assembly_read.len() == 1 {
                        &[0.20, 0.35, 0.50, 0.65, 0.80]
                    } else {
                        &[0.20, 0.40, 0.60, 0.80, 1.00, 1.20]
                    };
                    let candidate_origin = if defensive_roof {
                        // A defensive eave may already project clear of its
                        // supporting return. Prefer a direct drip from the
                        // physical low end before inventing a diagonal
                        // collector across the corner wall.
                        channel_low_plan - toward_channel_high * 0.10
                    } else {
                        desired_plan
                    };
                    let mut fall_candidates = along_offsets.iter().copied().flat_map(|along| {
                        outward_offsets.iter().copied().map(move |outward| {
                            candidate_origin + toward_channel_high * along + downhill * outward
                        })
                    });
                    let fall_plan = fall_candidates
                        .find(|candidate| {
                            let clears_solids = geometry.solids.iter().all(|solid| {
                                if solid.owner == assembly.owner
                                    && (solid.role == SolidRole::RoofEdgeTreatment
                                        || (solid.role == SolidRole::RoofGutter
                                            && source_channel_ids.contains(&solid.id)))
                                {
                                    return true;
                                }
                                if solid.role == SolidRole::RoofFace {
                                    return true;
                                }
                                let cosine = solid.yaw_radians.cos().abs();
                                let sine = solid.yaw_radians.sin().abs();
                                let half = Vec3::new(
                                    (solid.size.x * cosine + solid.size.z * sine) * 0.5,
                                    solid.size.y * 0.5,
                                    (solid.size.x * sine + solid.size.z * cosine) * 0.5,
                                );
                                let min = solid.centre - half;
                                let max = solid.centre + half;
                                let plan_hit = match solid.shape {
                                    crate::ResolvedSolidShape::RoundTowerShell {
                                        outer_radius_metres,
                                        ..
                                    } => {
                                        candidate
                                            .distance(Vec2::new(solid.centre.x, solid.centre.z))
                                            <= outer_radius_metres + 0.08
                                    }
                                    _ => {
                                        candidate.x >= min.x - 0.08
                                            && candidate.x <= max.x + 0.08
                                            && candidate.y >= min.z - 0.08
                                            && candidate.y <= max.z + 0.08
                                    }
                                };
                                let height_hit = outlet_y - 0.08 > min.y && 0.16 < max.y;
                                let outlet_cut_hits_other_gutter = solid.role
                                    == SolidRole::RoofGutter
                                    && candidate.x >= min.x - 0.05
                                    && candidate.x <= max.x + 0.05
                                    && candidate.y >= min.z - 0.05
                                    && candidate.y <= max.z + 0.05
                                    && outlet_y + 0.05 >= min.y
                                    && outlet_y - 0.05 <= max.y;
                                !(plan_hit && height_hit) && !outlet_cut_hits_other_gutter
                            });
                            let collector_start_y = source_network.channel_low.y - 0.025;
                            let source_channel_delta = Vec2::new(
                                source_network.channel_high.x - source_network.channel_low.x,
                                source_network.channel_high.z - source_network.channel_low.z,
                            );
                            let source_channel_t = ((*candidate - channel_low_plan)
                                .dot(source_channel_delta)
                                / source_channel_delta.length_squared().max(0.000_001))
                            .clamp(0.0, 1.0);
                            let on_source_channel = defensive_roof
                                && candidate.distance(
                                    channel_low_plan + source_channel_delta * source_channel_t,
                                ) <= 0.08;
                            let direct_drip = candidate.distance(channel_low_plan) <= 0.11;
                            let collector_clears_solids = direct_drip
                                || on_source_channel
                                || (1..10).all(|sample| {
                                    let fraction = sample as f32 / 10.0;
                                    let point = channel_low_plan.lerp(*candidate, fraction);
                                    let height = collector_start_y
                                        + (outlet_y - collector_start_y) * fraction;
                                    geometry.solids.iter().all(|solid| {
                                        if solid.owner == assembly.owner
                                            && matches!(
                                                solid.role,
                                                SolidRole::RoofGutter
                                                    | SolidRole::RoofEdgeTreatment
                                            )
                                        {
                                            return true;
                                        }
                                        if !matches!(
                                            solid.role,
                                            SolidRole::WallHost
                                                | SolidRole::OpeningJamb
                                                | SolidRole::OpeningSill
                                                | SolidRole::OpeningHead
                                                | SolidRole::OpeningSpandrel
                                                | SolidRole::RoofFlashing
                                        ) {
                                            return true;
                                        }
                                        let cosine = solid.yaw_radians.cos().abs();
                                        let sine = solid.yaw_radians.sin().abs();
                                        let half = Vec3::new(
                                            (solid.size.x * cosine + solid.size.z * sine) * 0.5,
                                            solid.size.y * 0.5,
                                            (solid.size.x * sine + solid.size.z * cosine) * 0.5,
                                        );
                                        point.x < solid.centre.x - half.x - 0.06
                                            || point.x > solid.centre.x + half.x + 0.06
                                            || point.y < solid.centre.z - half.z - 0.06
                                            || point.y > solid.centre.z + half.z + 0.06
                                            || height < solid.centre.y - half.y - 0.04
                                            || height > solid.centre.y + half.y + 0.04
                                    })
                                });
                            let clears_stairs = stairs.iter().all(|stair| match *stair {
                                Stair::Straight {
                                    start,
                                    direction,
                                    width_metres,
                                    tread_count,
                                    ..
                                } => {
                                    let axis = match direction {
                                        Direction::North => Vec2::Y,
                                        Direction::South => -Vec2::Y,
                                        Direction::East => Vec2::X,
                                        Direction::West => -Vec2::X,
                                    };
                                    let end = start + axis * tread_count as f32 * 0.28;
                                    let delta = end - start;
                                    let t = ((*candidate - start).dot(delta)
                                        / delta.length_squared().max(0.000_001))
                                    .clamp(0.0, 1.0);
                                    candidate.distance(start + delta * t)
                                        > width_metres * 0.5 + 0.30
                                }
                                Stair::Spiral {
                                    centre,
                                    outer_radius_metres,
                                    ..
                                } => candidate.distance(centre) > outer_radius_metres + 0.30,
                            });
                            let clears_portals = opening_voids
                                .iter()
                                .filter(|void| {
                                    matches!(
                                        void.role,
                                        VoidRole::WallOpening | VoidRole::AccessPortal
                                    ) && void.bounds.min.y < 1.08
                                })
                                .all(|void| {
                                    candidate.x < void.bounds.min.x - 0.30
                                        || candidate.x > void.bounds.max.x + 0.30
                                        || candidate.y < void.bounds.min.z - 0.30
                                        || candidate.y > void.bounds.max.z + 0.30
                                });
                            clears_solids
                                && collector_clears_solids
                                && clears_stairs
                                && clears_portals
                        })
                        .unwrap_or(
                            candidate_origin
                                + toward_channel_high * along_offsets[along_offsets.len() - 1]
                                + downhill * outward_offsets[outward_offsets.len() - 1],
                        );
                    let outlet = Vec3::new(fall_plan.x, outlet_y, fall_plan.y);
                    let discharge = Vec3::new(fall_plan.x, 0.08, fall_plan.y);
                    (
                        RoofDrainageDisposition::FreeDripToGround,
                        None,
                        None,
                        RoofDrainageRecipient::GroundSplashApron,
                        outlet,
                        discharge,
                        None,
                    )
                };

            geometry
                .solids
                .retain(|solid| !old_spouts.contains(&solid.id));
            geometry.support_interfaces.retain(|interface| {
                !old_spouts.iter().any(|id| {
                    interface.id == ResolvedItemId((0x9_u64 << 60) | (id.0 & 0x0FFF_FFFF_FFFF_FFFF))
                })
            });
            geometry
                .voids
                .retain(|void| !old_outlets.contains(&void.id));
            geometry.voids.push(ResolvedVoid {
                id: shared_outlet,
                owner: assembly.owner,
                bounds: ResolvedBounds {
                    min: outlet - Vec3::splat(0.045),
                    max: outlet + Vec3::splat(0.045),
                },
                role: VoidRole::Drain,
                shape: crate::ResolvedVoidShape::Box,
                subtracts_from: assembly.owner,
            });

            if let (Some(spout), Some(host_id)) = (spout_id, host_wall) {
                let host = walls
                    .iter()
                    .find(|wall| wall.id == host_id)
                    .expect("selected roof drain host");
                let height = (outlet.y - discharge.y - 0.14).max(0.09);
                let centre = Vec3::new(outlet.x, discharge.y + height * 0.5 + 0.07, outlet.z);
                geometry.solids.push(ResolvedSolid {
                    id: spout,
                    owner: assembly.owner,
                    centre,
                    size: Vec3::new(0.09, height, 0.09),
                    yaw_radians: 0.0,
                    crossfall_radians: 0.0,
                    longfall_radians: 0.0,
                    role: SolidRole::RoofGutter,
                    shape: crate::ResolvedSolidShape::Cuboid,
                    supported_by: vec![host.support_node],
                });
                geometry.support_interfaces.push(SupportInterface {
                    id: ResolvedItemId((0x9_u64 << 60) | (spout.0 & 0x0FFF_FFFF_FFFF_FFFF)),
                    owner: assembly.owner,
                    node: host.support_node,
                    bounds: ResolvedBounds {
                        min: centre - Vec3::splat(0.04),
                        max: centre + Vec3::splat(0.04),
                    },
                });
            }

            let mut member_networks = Vec::new();
            for (member_slot, index) in chunk.iter().copied().enumerate() {
                let network_id = geometry.roof_drainage_networks[index].id;
                member_networks.push(network_id);
                let start = geometry.roof_drainage_networks[index].channel_low - Vec3::Y * 0.025;
                let raw_delta = outlet - start;
                let plan_direction = Vec2::new(raw_delta.x, raw_delta.z).normalize_or_zero();
                // The collector terminates at the rim of the outlet cut rather
                // than occupying its free volume. The 0.10 m setback is the
                // project gutter-mouth radius plus a small construction joint.
                let collector_end =
                    outlet - Vec3::new(plan_direction.x * 0.10, 0.0, plan_direction.y * 0.10);
                let delta = collector_end - start;
                let plan_length = Vec2::new(delta.x, delta.z).length();
                let channel_low_plan = Vec2::new(
                    geometry.roof_drainage_networks[index].channel_low.x,
                    geometry.roof_drainage_networks[index].channel_low.z,
                );
                let channel_delta = Vec2::new(
                    geometry.roof_drainage_networks[index].channel_high.x
                        - geometry.roof_drainage_networks[index].channel_low.x,
                    geometry.roof_drainage_networks[index].channel_high.z
                        - geometry.roof_drainage_networks[index].channel_low.z,
                );
                let outlet_plan = Vec2::new(outlet.x, outlet.z);
                let channel_t = ((outlet_plan - channel_low_plan).dot(channel_delta)
                    / channel_delta.length_squared().max(0.000_001))
                .clamp(0.0, 1.0);
                let outlet_is_on_channel =
                    matches!(
                        archetype,
                        BuildingArchetype::CastleGatehouse | BuildingArchetype::CourtyardCastle
                    ) && outlet_plan.distance(channel_low_plan + channel_delta * channel_t) <= 0.08;
                let mut collectors = Vec::new();
                if plan_length > 0.10 && !outlet_is_on_channel {
                    let collector = ResolvedItemId(
                        (0x8_u64 << 60)
                            | (assembly.id.0 << 16)
                            | 0x7800
                            | ((station_slot as u64 & 0x7) << 5)
                            | (member_slot as u64 & 0x1F),
                    );
                    let face = assembly
                        .faces
                        .iter()
                        .find(|face| face.id == geometry.roof_drainage_networks[index].face)
                        .expect("drainage face");
                    let compact_child_collector = assembly.parent.is_some()
                        && matches!(assembly.kind, RoofKind::Gable | RoofKind::Shed);
                    geometry.solids.push(ResolvedSolid {
                        id: collector,
                        owner: assembly.owner,
                        centre: (start + collector_end) * 0.5,
                        size: Vec3::new(
                            plan_length,
                            if compact_child_collector {
                                0.018
                            } else {
                                0.035
                            },
                            if compact_child_collector { 0.070 } else { 0.12 },
                        ),
                        yaw_radians: delta.z.atan2(delta.x),
                        crossfall_radians: 0.0,
                        longfall_radians: delta.y.atan2(plan_length),
                        role: SolidRole::RoofGutter,
                        shape: crate::ResolvedSolidShape::Cuboid,
                        supported_by: face.support_nodes.clone(),
                    });
                    geometry.support_interfaces.push(SupportInterface {
                        id: ResolvedItemId((0x9_u64 << 60) | (collector.0 & 0x0FFF_FFFF_FFFF_FFFF)),
                        owner: assembly.owner,
                        node: face.support_nodes[0],
                        bounds: ResolvedBounds {
                            min: start - Vec3::splat(0.035),
                            max: start + Vec3::splat(0.035),
                        },
                    });
                    collectors.push(collector);
                }
                let network = &mut geometry.roof_drainage_networks[index];
                if let Some(edge) = assemblies
                    .iter_mut()
                    .find(|roof| roof.id == assembly.id)
                    .and_then(|roof| {
                        roof.edges
                            .iter_mut()
                            .find(|edge| edge.id == network.receiving_edge)
                    })
                {
                    edge.drainage_terminal = Some(shared_outlet);
                }
                network.collector_solids = collectors.clone();
                network.outlet_station = station_id;
                network.outlet_void = shared_outlet;
                network.downspout = spout_id;
                network.discharge = discharge;
                if let Some(catchment) = geometry
                    .drainage_catchments
                    .iter_mut()
                    .find(|catchment| catchment.id == network.catchment)
                {
                    catchment.toe_channel_solids.extend(collectors);
                    if let Some(route) = geometry
                        .drainage_routes
                        .iter_mut()
                        .find(|route| route.id == catchment.outlet_route)
                    {
                        route.outlet_void = shared_outlet;
                        route.outlet = outlet;
                    }
                }
            }
            let recipient_bounds = ResolvedBounds {
                min: discharge - Vec3::new(0.30, 0.03, 0.30),
                max: discharge + Vec3::new(0.30, 0.03, 0.30),
            };
            geometry.surfaces.push(ResolvedSurface {
                id: recipient_surface,
                owner: assembly.owner,
                bounds: recipient_bounds,
                role: SurfaceRole::DrainageRecipient,
                shape: crate::ResolvedSurfaceShape::Planar,
            });
            geometry
                .roof_drainage_outlets
                .push(RoofDrainageOutletStation {
                    id: station_id,
                    owner: assembly.owner,
                    disposition,
                    member_networks,
                    host_wall,
                    facade_contact,
                    outlet_void: shared_outlet,
                    downspout: spout_id,
                    recipient,
                    recipient_surface,
                    discharge,
                });
        }
    }
}

fn resolve_roof_abutment_contours(
    assemblies: &mut [RoofAssembly],
    walls: &[crate::WallAssembly],
    geometry: &mut ResolvedGeometry,
) {
    for assembly in assemblies {
        for (kind_slot, (edge_kind, abutment_kind)) in [
            (RoofEdgeKind::WallAbutment, RoofAbutmentKind::Wall),
            (RoofEdgeKind::TowerAbutment, RoofAbutmentKind::Tower),
        ]
        .into_iter()
        .enumerate()
        {
            let edge_indices = assembly
                .edges
                .iter()
                .enumerate()
                .filter_map(|(index, edge)| (edge.kind == edge_kind).then_some(index))
                .collect::<Vec<_>>();
            if edge_indices.is_empty() {
                continue;
            }
            let abutment_id =
                ResolvedItemId((0x7_u64 << 60) | (assembly.id.0 << 16) | 0xD000 | kind_slot as u64);
            let mut samples = Vec::new();
            let mut edge_ids = Vec::new();
            for edge_index in edge_indices {
                let edge = &mut assembly.edges[edge_index];
                let old_flashing = edge.flashing;
                let first_edge_sample = samples.len();
                let first_edge_bond = geometry.junction_bonds.len();
                let delta = edge.end - edge.start;
                let length = delta.length().max(0.01);
                let station_count = (length / 0.22).ceil().max(1.0) as usize;
                let horizontal = Vec2::new(delta.x, delta.z).normalize_or_zero();
                for station in 0..=station_count {
                    let t = station as f32 / station_count as f32;
                    let point = edge.start.lerp(edge.end, t);
                    let plan_point = Vec2::new(point.x, point.z);
                    let host = walls
                        .iter()
                        .filter(|wall| {
                            if abutment_kind == RoofAbutmentKind::Tower {
                                matches!(wall.source, crate::WallSourceId::SquareTowerFace { .. })
                            } else {
                                !matches!(wall.source, crate::WallSourceId::RoofChildFront { .. })
                            }
                        })
                        .filter_map(|wall| {
                            let offset = plan_point - wall.frame.origin;
                            let signed_normal = offset.dot(wall.frame.outward);
                            // A weatherable roof abutment lies on the exterior
                            // masonry face, never the wall centreline. Clipped
                            // fragments on the interior side are opening-cut
                            // boundaries, not valid contact contours.
                            let normal_distance =
                                (signed_normal - wall.thickness_metres * 0.5).abs();
                            let along = offset.dot(wall.frame.tangent).abs();
                            let corner_return = if abutment_kind == RoofAbutmentKind::Tower {
                                wall.thickness_metres * 0.5
                            } else {
                                0.0
                            };
                            (normal_distance <= wall.thickness_metres * 0.5 + 0.18
                                && along <= wall.length_metres * 0.5 + corner_return + 0.18
                                && point.y >= wall.base_elevation_metres - 0.08
                                && point.y
                                    <= wall.base_elevation_metres + wall.height_metres + 0.18)
                                .then_some((wall, normal_distance))
                        })
                        .min_by(|(left_wall, left), (right_wall, right)| {
                            let priority = |wall: &crate::WallAssembly| {
                                if abutment_kind == RoofAbutmentKind::Wall
                                    && matches!(
                                        wall.source,
                                        crate::WallSourceId::ChurchArcade { .. }
                                    )
                                {
                                    0_u8
                                } else {
                                    1_u8
                                }
                            };
                            priority(left_wall)
                                .cmp(&priority(right_wall))
                                .then_with(|| left.total_cmp(right))
                        });
                    let Some((host, _)) = host else { continue };
                    if samples.len() == first_edge_sample
                        && let Some(old) = old_flashing
                    {
                        geometry.solids.retain(|solid| solid.id != old);
                    }
                    // Reserve independent bit ranges for edge and station.
                    // Long clerestory contacts exceed 64 samples, so the old
                    // `edge << 8 | station * 4` encoding aliased IDs once the
                    // station carried into the edge bits.
                    let serial = ((edge_index as u64 & 0x7) << 10) | ((station as u64 & 0xFF) << 2);
                    let base = (0x8_u64 << 60) | (assembly.id.0 << 16) | 0xA000 | serial;
                    let apron = if station == 0 {
                        old_flashing.unwrap_or(ResolvedItemId(base))
                    } else {
                        ResolvedItemId(base)
                    };
                    let upstand = ResolvedItemId(base | 1);
                    let counter = ResolvedItemId(base | 2);
                    let outward = Vec3::new(host.frame.outward.x, 0.0, host.frame.outward.y);
                    let tangent_yaw = horizontal.y.atan2(horizontal.x);
                    let span = (length / station_count as f32 + 0.035).max(0.12);
                    for (id, centre, size, crossfall) in [
                        (
                            apron,
                            // The apron lies on the roof side of the masonry
                            // contour and laps the host by roughly 70 mm.  It
                            // must not be centred inside the tower shell.
                            point + outward * 0.10 + Vec3::Y * 0.022,
                            Vec3::new(span, 0.045, 0.34),
                            -0.10,
                        ),
                        (
                            upstand,
                            point + outward * 0.012 + Vec3::Y * 0.18,
                            Vec3::new(span, 0.36, 0.055),
                            0.0,
                        ),
                        (
                            counter,
                            point + outward * 0.018 + Vec3::Y * 0.315,
                            Vec3::new(span, 0.12, 0.075),
                            0.0,
                        ),
                    ] {
                        geometry.solids.push(ResolvedSolid {
                            id,
                            owner: assembly.owner,
                            centre,
                            size,
                            yaw_radians: tangent_yaw,
                            crossfall_radians: crossfall,
                            longfall_radians: 0.0,
                            role: SolidRole::RoofFlashing,
                            shape: crate::ResolvedSolidShape::Cuboid,
                            supported_by: vec![host.support_node],
                        });
                        geometry.support_interfaces.push(SupportInterface {
                            id: ResolvedItemId((0x9_u64 << 60) | (id.0 & 0x0FFF_FFFF_FFFF_FFFF)),
                            owner: assembly.owner,
                            node: host.support_node,
                            bounds: ResolvedBounds {
                                min: centre - size * 0.18,
                                max: centre + size * 0.18,
                            },
                        });
                    }
                    samples.push(RoofAbutmentSample {
                        point,
                        host_wall: host.id,
                        apron_solid: apron,
                        upstand_solid: upstand,
                        counterflashing_solid: counter,
                    });
                    // At a tower corner one weathering strip can bear on both
                    // adjoining wall-face assemblies.  Declare every measured
                    // positive interface instead of assigning the whole strip
                    // to whichever face happened to win the nearest-host query.
                    let weather_ids = [apron, upstand, counter];
                    let rotated_half_extents = |solid: &ResolvedSolid| {
                        let cosine = solid.yaw_radians.cos().abs();
                        let sine = solid.yaw_radians.sin().abs();
                        Vec3::new(
                            (solid.size.x * cosine + solid.size.z * sine) * 0.5,
                            solid.size.y * 0.5,
                            (solid.size.x * sine + solid.size.z * cosine) * 0.5,
                        )
                    };
                    let bonded_hosts = walls
                        .iter()
                        .filter(|candidate| {
                            // Jambs, heads, and spandrels remain pieces of the
                            // authoritative wall owner even though they are not
                            // included in `host_solids`.  Bind weathering to
                            // every resolved piece it physically contacts.
                            geometry
                                .solids
                                .iter()
                                .filter(|solid| solid.owner == candidate.owner)
                                .any(|host_solid| {
                                    let host_half = rotated_half_extents(host_solid);
                                    weather_ids.iter().any(|weather_id| {
                                        let weather = geometry
                                            .solids
                                            .iter()
                                            .find(|solid| solid.id == *weather_id)
                                            .expect("new roof weathering solid must resolve");
                                        let weather_half = rotated_half_extents(weather);
                                        let overlap_min = (host_solid.centre - host_half)
                                            .max(weather.centre - weather_half);
                                        let overlap_max = (host_solid.centre + host_half)
                                            .min(weather.centre + weather_half);
                                        (overlap_max - overlap_min).min_element() > 0.025
                                    })
                                })
                        })
                        .map(|candidate| candidate.owner)
                        .collect::<BTreeSet<_>>();
                    for bonded_owner in bonded_hosts {
                        geometry.junction_bonds.push(JunctionBond {
                            id: ResolvedItemId(
                                (0x6_u64 << 60)
                                    | (assembly.id.0 << 32)
                                    | ((edge_index as u64) << 24)
                                    | ((station as u64) << 12)
                                    | (u64::from(bonded_owner.0) & 0xFFF),
                            ),
                            owners: [assembly.owner, bonded_owner],
                            bounds: ResolvedBounds {
                                min: point - Vec3::new(0.40, 0.25, 0.40),
                                max: point + Vec3::new(0.40, 0.40, 0.40),
                            },
                            minimum_interface_area_square_metres: 0.0005,
                            maximum_penetration_metres: 0.50,
                        });
                    }
                }
                if samples.len() - first_edge_sample == station_count + 1 {
                    edge_ids.push(edge.id);
                    edge.flashing = samples.last().map(|sample| sample.apron_solid);
                } else {
                    // A parent-face subdivision edge inside the tower footprint
                    // is part of the opening cut, not part of the masonry contact
                    // contour.  It owns neither an upstand nor counterflashing.
                    if let Some(old) = old_flashing {
                        geometry.solids.retain(|solid| solid.id != old);
                    }
                    let rejected = samples
                        .drain(first_edge_sample..)
                        .flat_map(|sample| {
                            [
                                sample.apron_solid,
                                sample.upstand_solid,
                                sample.counterflashing_solid,
                            ]
                        })
                        .collect::<HashSet<_>>();
                    geometry
                        .solids
                        .retain(|solid| !rejected.contains(&solid.id));
                    geometry.support_interfaces.retain(|interface| {
                        !rejected.iter().any(|id| {
                            interface.id
                                == ResolvedItemId((0x9_u64 << 60) | (id.0 & 0x0FFF_FFFF_FFFF_FFFF))
                        })
                    });
                    geometry.junction_bonds.truncate(first_edge_bond);
                    edge.kind = RoofEdgeKind::OpeningCut;
                    edge.flashing = None;
                }
            }
            if samples.is_empty() {
                continue;
            }
            let lower_sample = samples
                .iter()
                .min_by(|left, right| left.point.y.total_cmp(&right.point.y))
                .expect("non-empty abutment samples");
            let lower = lower_sample.point;
            let lower_outward = walls
                .iter()
                .find(|wall| wall.id == lower_sample.host_wall)
                .map(|wall| Vec3::new(wall.frame.outward.x, 0.0, wall.frame.outward.y))
                .unwrap_or(Vec3::Z);
            let outlet_point = lower + lower_outward * 0.30 - Vec3::Y * 0.08;
            let outlet =
                ResolvedItemId((0xE_u64 << 60) | (assembly.id.0 << 16) | 0xD000 | kind_slot as u64);
            let route =
                ResolvedItemId((0xD_u64 << 60) | (assembly.id.0 << 16) | 0xD000 | kind_slot as u64);
            geometry.voids.push(ResolvedVoid {
                id: outlet,
                owner: assembly.owner,
                bounds: ResolvedBounds {
                    min: outlet_point - Vec3::splat(0.055),
                    max: outlet_point + Vec3::splat(0.055),
                },
                role: VoidRole::Drain,
                shape: crate::ResolvedVoidShape::Box,
                subtracts_from: assembly.owner,
            });
            geometry.drainage_routes.push(DrainageRoute {
                id: route,
                owner: assembly.owner,
                outlet_void: outlet,
                inlet: samples
                    .iter()
                    .max_by(|left, right| left.point.y.total_cmp(&right.point.y))
                    .expect("non-empty abutment samples")
                    .point
                    + Vec3::Y * 0.03,
                outlet: outlet_point,
            });
            assembly.abutments.push(RoofAbutmentAssembly {
                id: abutment_id,
                kind: abutment_kind,
                edge_ids: edge_ids.clone(),
                samples,
                lower_outlet: outlet,
                drainage_route: route,
            });
            for child in &mut assembly.children {
                if child.kind == RoofChildKind::Tower {
                    child.valley_edges.retain(|id| edge_ids.contains(id));
                    child.flashing_ids = child
                        .valley_edges
                        .iter()
                        .filter_map(|id| {
                            assembly
                                .edges
                                .iter()
                                .find(|edge| edge.id == *id)
                                .and_then(|edge| edge.flashing)
                        })
                        .collect();
                }
            }
        }
    }
}

fn split_cross_gable_parent_eave(
    parent: &mut RoofAssembly,
    child_id: RoofAssemblyId,
    origin: Vec2,
    tangent: Vec2,
    width: f32,
) -> Vec<ResolvedItemId> {
    let front_left = origin - tangent * width * 0.5;
    let front_right = origin + tangent * width * 0.5;
    let candidate = parent
        .edges
        .iter()
        .enumerate()
        .find_map(|(edge_index, edge)| {
            if edge.kind != RoofEdgeKind::Eave {
                return None;
            }
            let a = Vec2::new(edge.start.x, edge.start.z);
            let b = Vec2::new(edge.end.x, edge.end.z);
            let delta = b - a;
            if delta.normalize_or_zero().dot(tangent).abs() < 0.99 {
                return None;
            }
            let denominator = delta.length_squared();
            let left_t = (front_left - a).dot(delta) / denominator.max(0.000_001);
            let right_t = (front_right - a).dot(delta) / denominator.max(0.000_001);
            let lo = left_t.min(right_t);
            let hi = left_t.max(right_t);
            (lo > 0.02 && hi < 0.98 && (hi - lo) * delta.length() >= width * 0.85)
                .then_some((edge_index, lo, hi))
        });
    let Some((edge_index, lo, hi)) = candidate else {
        return Vec::new();
    };
    let old = parent.edges.remove(edge_index);
    let left_point = old.start.lerp(old.end, lo);
    let right_point = old.start.lerp(old.end, hi);
    let serial = parent.edges.len() as u64;
    let ids = [0_u64, 1, 2].map(|slot| {
        ResolvedItemId((0xB_u64 << 60) | (parent.id.0 << 16) | 0x0E00 | (serial << 2) | slot)
    });
    parent.edges.extend([
        RoofEdge {
            id: ids[0],
            start: old.start,
            end: left_point,
            kind: RoofEdgeKind::Eave,
            adjacent_faces: old.adjacent_faces.clone(),
            flashing: None,
            drainage_terminal: old.drainage_terminal,
        },
        RoofEdge {
            id: ids[1],
            start: left_point,
            end: right_point,
            kind: RoofEdgeKind::OpeningCut,
            adjacent_faces: old.adjacent_faces.clone(),
            flashing: None,
            drainage_terminal: None,
        },
        RoofEdge {
            id: ids[2],
            start: right_point,
            end: old.end,
            kind: RoofEdgeKind::Eave,
            adjacent_faces: old.adjacent_faces,
            flashing: None,
            drainage_terminal: old.drainage_terminal,
        },
    ]);
    if let Some(link) = parent
        .children
        .iter_mut()
        .find(|link| link.child == child_id)
    {
        link.split_eave_edges = ids.to_vec();
    }
    ids.to_vec()
}

fn clip_plan_polygon_to_child_above_parent(
    mut polygon: Vec<Vec2>,
    parent: RoofPlaneEquation,
    child: RoofPlaneEquation,
) -> Vec<Vec2> {
    if polygon.is_empty() {
        return polygon;
    }
    let clearance =
        |point: Vec2| roof_plane_height(child, point) - roof_plane_height(parent, point);
    let input = std::mem::take(&mut polygon);
    for index in 0..input.len() {
        let current = input[index];
        let previous = input[(index + input.len() - 1) % input.len()];
        let current_clearance = clearance(current);
        let previous_clearance = clearance(previous);
        let current_inside = current_clearance >= -0.001;
        let previous_inside = previous_clearance >= -0.001;
        if current_inside != previous_inside {
            let denominator = previous_clearance - current_clearance;
            let fraction = if denominator.abs() <= 0.000_001 {
                0.0
            } else {
                previous_clearance / denominator
            };
            polygon.push(previous.lerp(current, fraction));
        }
        if current_inside {
            polygon.push(current);
        }
    }
    polygon
}

fn cut_parent_roof_face(
    assembly: &mut RoofAssembly,
    child: &RoofAssembly,
    cut_bounds: ResolvedBounds,
    geometry: &mut ResolvedGeometry,
) -> Vec<ResolvedItemId> {
    let mut cut_edges = Vec::new();
    let serial_base = assembly.edges.len();
    for face in &mut assembly.faces {
        let projected = face
            .polygon
            .iter()
            .map(|point| Vec2::new(point.x, point.z))
            .collect::<Vec<_>>();
        let mut cut_points = Vec::new();
        for child_face in &child.faces {
            let child_projected = child_face
                .polygon
                .iter()
                .map(|point| Vec2::new(point.x, point.z))
                .collect::<Vec<_>>();
            let bounded = clip_plan_polygon_to_rect(
                projected.clone(),
                Vec2::new(cut_bounds.min.x, cut_bounds.min.z),
                Vec2::new(cut_bounds.max.x, cut_bounds.max.z),
            );
            let mut clipped = clip_plan_polygon_to_convex(bounded, &child_projected);
            clipped =
                clip_plan_polygon_to_child_above_parent(clipped, face.plane, child_face.plane);
            let mut unique = Vec::new();
            for point in clipped {
                if !unique
                    .iter()
                    .any(|existing: &Vec2| existing.distance_squared(point) <= 0.000_004)
                {
                    unique.push(point);
                }
            }
            cut_points.extend(unique);
        }
        let unique = convex_plan_hull(cut_points);
        let signed_area = signed_plan_area(&unique);
        if unique.len() >= 3 && signed_area.abs() > 0.002 {
            let mut cutout = unique
                .into_iter()
                .map(|point| Vec3::new(point.x, roof_plane_height(face.plane, point), point.y))
                .collect::<Vec<_>>();
            if signed_area > 0.0 {
                cutout.reverse();
            }
            let face_id = face.id;
            face.cutouts.push(cutout.clone());
            for index in 0..cutout.len() {
                let serial = serial_base + cut_edges.len();
                let edge_id = ResolvedItemId(
                    (0xB_u64 << 60) | (assembly.id.0 << 16) | (0x800 + serial) as u64,
                );
                let start = cutout[index];
                let end = cutout[(index + 1) % cutout.len()];
                let flashing_id = ResolvedItemId(
                    (0x8_u64 << 60) | (assembly.id.0 << 16) | (0x800 + serial) as u64,
                );
                let delta = end - start;
                geometry.solids.push(ResolvedSolid {
                    id: flashing_id,
                    owner: assembly.owner,
                    centre: (start + end) * 0.5 + Vec3::Y * 0.012,
                    // Leave a physical 80 mm outlet throat at each junction;
                    // an unbroken flashing bar would seal the valley terminal.
                    size: Vec3::new((delta.length() - 0.12).max(0.05), 0.024, 0.10),
                    yaw_radians: delta.z.atan2(delta.x),
                    crossfall_radians: 0.08,
                    longfall_radians: 0.0,
                    role: SolidRole::RoofFlashing,
                    shape: crate::ResolvedSolidShape::Cuboid,
                    supported_by: vec![assembly.support_nodes[0]],
                });
                geometry.support_interfaces.push(SupportInterface {
                    id: ResolvedItemId(
                        (0x9_u64 << 60) | (assembly.id.0 << 16) | (0x800 + serial) as u64,
                    ),
                    owner: assembly.owner,
                    node: assembly.support_nodes[0],
                    bounds: ResolvedBounds {
                        min: (start + end) * 0.5 - Vec3::new(0.12, 0.04, 0.12),
                        max: (start + end) * 0.5 + Vec3::new(0.12, 0.08, 0.12),
                    },
                });
                cut_edges.push(edge_id);
                assembly.edges.push(RoofEdge {
                    id: edge_id,
                    start,
                    end,
                    kind: RoofEdgeKind::OpeningCut,
                    adjacent_faces: vec![face_id],
                    flashing: Some(flashing_id),
                    drainage_terminal: None,
                });
            }
        }
    }
    cut_edges
}

fn bind_child_valleys(
    parent: &mut RoofAssembly,
    child: &RoofAssembly,
    cut_edges: &[ResolvedItemId],
    geometry: &mut ResolvedGeometry,
) -> Vec<ResolvedItemId> {
    let mut valleys = Vec::new();
    let mut candidates = cut_edges
        .iter()
        .filter_map(|id| {
            parent
                .edges
                .iter()
                .find(|edge| edge.id == *id)
                .map(|edge| (*id, (edge.start.y - edge.end.y).abs()))
        })
        .filter(|(_, fall)| *fall > 0.02)
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| right.1.total_cmp(&left.1));
    for (edge_id, _) in candidates.into_iter().take(2) {
        if let Some(edge) = parent.edges.iter_mut().find(|edge| edge.id == edge_id) {
            edge.kind = RoofEdgeKind::Valley;
            let (high, low) = if edge.start.y >= edge.end.y {
                (edge.start, edge.end)
            } else {
                (edge.end, edge.start)
            };
            let suffix = edge.id.0 & ((1_u64 << 60) - 1);
            let outlet_id = ResolvedItemId((0xE_u64 << 60) | suffix);
            let route_id = ResolvedItemId((0xD_u64 << 60) | suffix);
            geometry.voids.push(ResolvedVoid {
                id: outlet_id,
                owner: parent.owner,
                bounds: ResolvedBounds {
                    // The terminal is an upward-open throat beginning at the
                    // weather surface. Extending it below the valley would
                    // falsely cut the receiving eave gutter or wall plate.
                    min: low - Vec3::new(0.04, 0.0, 0.04),
                    max: low + Vec3::new(0.04, 0.08, 0.04),
                },
                role: VoidRole::Drain,
                shape: crate::ResolvedVoidShape::Box,
                subtracts_from: parent.owner,
            });
            geometry.drainage_routes.push(DrainageRoute {
                id: route_id,
                owner: parent.owner,
                outlet_void: outlet_id,
                inlet: high,
                outlet: low,
            });
            edge.drainage_terminal = Some(outlet_id);
            if let Some(flashing) = edge
                .flashing
                .and_then(|id| geometry.solids.iter_mut().find(|solid| solid.id == id))
            {
                let delta = edge.end - edge.start;
                let run = Vec2::new(delta.x, delta.z).length().max(0.01);
                let uphill = (high - low).normalize_or_zero();
                flashing.centre =
                    (high + low) * 0.5 + uphill * 0.06 + Vec3::Y * (flashing.size.y * 0.5);
                flashing.size.x = (delta.length() - 0.12).max(0.05);
                flashing.longfall_radians = delta.y.atan2(run);
            }
            if let Some(face) = child.faces.iter().min_by(|left, right| {
                let midpoint = (edge.start + edge.end) * 0.5;
                let left_distance = left
                    .polygon
                    .iter()
                    .map(|point| point.distance_squared(midpoint))
                    .fold(f32::INFINITY, f32::min);
                let right_distance = right
                    .polygon
                    .iter()
                    .map(|point| point.distance_squared(midpoint))
                    .fold(f32::INFINITY, f32::min);
                left_distance.total_cmp(&right_distance)
            }) {
                edge.adjacent_faces.push(face.id);
            }
            valleys.push(edge_id);
        }
    }
    valleys
}

fn trim_roof_edge_treatments_for_cut(
    owner: GeometryOwnerId,
    cut: ResolvedBounds,
    geometry: &mut ResolvedGeometry,
) {
    let cut_centre = Vec2::new((cut.min.x + cut.max.x) * 0.5, (cut.min.z + cut.max.z) * 0.5);
    let cut_half = Vec2::new((cut.max.x - cut.min.x) * 0.5, (cut.max.z - cut.min.z) * 0.5);
    for solid in geometry.solids.iter_mut().filter(|solid| {
        solid.owner == owner
            && matches!(
                solid.role,
                SolidRole::RoofEdgeTreatment | SolidRole::RoofGutter
            )
    }) {
        let tangent = Vec2::new(solid.yaw_radians.cos(), solid.yaw_radians.sin());
        let plan_scale = solid.longfall_radians.cos().abs().max(0.01);
        let normal = Vec2::new(-tangent.y, tangent.x);
        let centre = Vec2::new(solid.centre.x, solid.centre.z);
        let offset = cut_centre - centre;
        let lateral_extent = cut_half.x * normal.x.abs() + cut_half.y * normal.y.abs();
        if offset.dot(normal).abs() > lateral_extent + solid.size.z * 0.5 + 0.01 {
            continue;
        }
        let cut_along = (cut_half.x * tangent.x.abs() + cut_half.y * tangent.y.abs()) / plan_scale;
        let cut_centre_along = offset.dot(tangent) / plan_scale;
        let cut_min = cut_centre_along - cut_along;
        let cut_max = cut_centre_along + cut_along;
        let old_min = -solid.size.x * 0.5;
        let old_max = solid.size.x * 0.5;
        if cut_max <= old_min + 0.01 || cut_min >= old_max - 0.01 {
            continue;
        }
        let kept = if cut_min <= old_min + 0.01 && cut_max < old_max - 0.05 {
            Some((cut_max + 0.08, old_max))
        } else if cut_max >= old_max - 0.01 && cut_min > old_min + 0.05 {
            Some((old_min, cut_min - 0.08))
        } else {
            // No curated tower presently bisects a ridge. Rejecting this
            // topology later is safer than leaving a treatment through the
            // opening; a full two-segment edge graph is required for it.
            None
        };
        if let Some((from, to)) = kept {
            let shift = (from + to) * 0.5;
            solid.centre.x += tangent.x * plan_scale * shift;
            solid.centre.y += solid.longfall_radians.sin() * shift;
            solid.centre.z += tangent.y * plan_scale * shift;
            solid.size.x = to - from;
            let interface_id = ResolvedItemId((0x9_u64 << 60) | (solid.id.0 & ((1_u64 << 60) - 1)));
            if let Some(interface) = geometry
                .support_interfaces
                .iter_mut()
                .find(|interface| interface.id == interface_id)
            {
                interface.bounds.min = solid.centre - Vec3::new(0.08, 0.025, 0.08);
                interface.bounds.max = solid.centre + Vec3::new(0.08, 0.025, 0.08);
            }
        } else {
            solid.size.x = 0.0;
        }
    }
    geometry.solids.retain(|solid| solid.size.x > 0.001);
}

fn trim_roof_boundary_edges_for_cut(assembly: &mut RoofAssembly, cut: ResolvedBounds) {
    let inside = |point: Vec3| {
        point.x >= cut.min.x - 0.002
            && point.x <= cut.max.x + 0.002
            && point.z >= cut.min.z - 0.002
            && point.z <= cut.max.z + 0.002
    };
    for edge in assembly.edges.iter_mut().filter(|edge| {
        matches!(edge.kind, RoofEdgeKind::Eave | RoofEdgeKind::GableVerge)
            && edge.adjacent_faces.len() == 1
    }) {
        let start_inside = inside(edge.start);
        let end_inside = inside(edge.end);
        if start_inside == end_inside {
            continue;
        }
        let from = edge.start;
        let delta = edge.end - edge.start;
        let mut intersections = Vec::new();
        for (axis_start, axis_delta, low, high) in [
            (from.x, delta.x, cut.min.x, cut.max.x),
            (from.z, delta.z, cut.min.z, cut.max.z),
        ] {
            if axis_delta.abs() <= 0.000_001 {
                continue;
            }
            for boundary in [low, high] {
                let t = (boundary - axis_start) / axis_delta;
                if (0.0..=1.0).contains(&t) {
                    let point = from + delta * t;
                    if point.x >= cut.min.x - 0.003
                        && point.x <= cut.max.x + 0.003
                        && point.z >= cut.min.z - 0.003
                        && point.z <= cut.max.z + 0.003
                    {
                        intersections.push((t, point));
                    }
                }
            }
        }
        if start_inside {
            if let Some((_, point)) = intersections
                .into_iter()
                .max_by(|left, right| left.0.total_cmp(&right.0))
            {
                edge.start = point + delta.normalize_or_zero() * 0.10;
            }
        } else if let Some((_, point)) = intersections
            .into_iter()
            .min_by(|left, right| left.0.total_cmp(&right.0))
        {
            edge.end = point - delta.normalize_or_zero() * 0.10;
        }
    }
}

fn bind_coincident_primary_roof_edges(
    assemblies: &mut [RoofAssembly],
    geometry: &mut ResolvedGeometry,
) {
    let mut removals = Vec::new();
    for left_index in 0..assemblies.len() {
        if assemblies[left_index].parent.is_some()
            || assemblies[left_index].source_piece_index.is_none()
        {
            continue;
        }
        for right_index in left_index + 1..assemblies.len() {
            if assemblies[right_index].parent.is_some()
                || assemblies[right_index].source_piece_index.is_none()
            {
                continue;
            }
            for left_edge_index in 0..assemblies[left_index].edges.len() {
                let left_edge = assemblies[left_index].edges[left_edge_index].clone();
                if left_edge.adjacent_faces.len() != 1 {
                    continue;
                }
                let Some((right_edge_index, right_edge)) = assemblies[right_index]
                    .edges
                    .iter()
                    .enumerate()
                    .find(|(_, edge)| {
                        edge.adjacent_faces.len() == 1
                            && ((same_roof_vertex(left_edge.start, edge.end)
                                && same_roof_vertex(left_edge.end, edge.start))
                                || (same_roof_vertex(left_edge.start, edge.start)
                                    && same_roof_vertex(left_edge.end, edge.end)))
                    })
                    .map(|(index, edge)| (index, edge.clone()))
                else {
                    continue;
                };
                let kind = if matches!(
                    left_edge.kind,
                    RoofEdgeKind::WallAbutment | RoofEdgeKind::TowerAbutment
                ) || matches!(
                    right_edge.kind,
                    RoofEdgeKind::WallAbutment | RoofEdgeKind::TowerAbutment
                ) {
                    RoofEdgeKind::WallAbutment
                } else {
                    RoofEdgeKind::Valley
                };
                let flashing_id = ResolvedItemId(
                    (0x8_u64 << 60)
                        | (assemblies[left_index].id.0 << 16)
                        | 0x6000
                        | left_edge_index as u64,
                );
                let delta = left_edge.end - left_edge.start;
                let support = assemblies[left_index].support_nodes[0];
                geometry.solids.push(ResolvedSolid {
                    id: flashing_id,
                    owner: assemblies[left_index].owner,
                    centre: (left_edge.start + left_edge.end) * 0.5 + Vec3::Y * 0.02,
                    size: Vec3::new(Vec2::new(delta.x, delta.z).length(), 0.06, 0.20),
                    yaw_radians: delta.z.atan2(delta.x),
                    crossfall_radians: if kind == RoofEdgeKind::Valley {
                        -0.08
                    } else {
                        0.12
                    },
                    longfall_radians: if kind == RoofEdgeKind::Valley {
                        0.012
                    } else {
                        0.0
                    },
                    role: SolidRole::RoofFlashing,
                    shape: crate::ResolvedSolidShape::Cuboid,
                    supported_by: vec![support],
                });
                geometry.support_interfaces.push(SupportInterface {
                    id: ResolvedItemId(
                        (0x9_u64 << 60)
                            | (assemblies[left_index].id.0 << 16)
                            | 0x6000
                            | left_edge_index as u64,
                    ),
                    owner: assemblies[left_index].owner,
                    node: support,
                    bounds: ResolvedBounds {
                        min: (left_edge.start + left_edge.end) * 0.5 - Vec3::new(0.08, 0.025, 0.08),
                        max: (left_edge.start + left_edge.end) * 0.5 + Vec3::new(0.08, 0.025, 0.08),
                    },
                });
                let edge = &mut assemblies[left_index].edges[left_edge_index];
                edge.kind = kind;
                edge.flashing = Some(flashing_id);
                edge.adjacent_faces.push(right_edge.adjacent_faces[0]);
                if kind == RoofEdgeKind::Valley {
                    let outlet_id = ResolvedItemId(
                        (0xE_u64 << 60)
                            | (assemblies[left_index].id.0 << 16)
                            | 0x6000
                            | left_edge_index as u64,
                    );
                    let outlet = left_edge.end + delta.normalize_or_zero() * 0.08 - Vec3::Y * 0.08;
                    geometry.voids.push(ResolvedVoid {
                        id: outlet_id,
                        owner: assemblies[left_index].owner,
                        bounds: ResolvedBounds {
                            min: outlet - Vec3::splat(0.04),
                            max: outlet + Vec3::splat(0.04),
                        },
                        role: VoidRole::Drain,
                        shape: crate::ResolvedVoidShape::Box,
                        subtracts_from: assemblies[left_index].owner,
                    });
                    let route_id = ResolvedItemId(
                        (0xD_u64 << 60)
                            | (assemblies[left_index].id.0 << 16)
                            | 0x6000
                            | left_edge_index as u64,
                    );
                    geometry.drainage_routes.push(DrainageRoute {
                        id: route_id,
                        owner: assemblies[left_index].owner,
                        outlet_void: outlet_id,
                        inlet: left_edge.start + Vec3::Y * 0.02,
                        outlet,
                    });
                    edge.drainage_terminal = Some(outlet_id);
                }
                let weather_ids = [
                    ResolvedItemId(
                        (0x8_u64 << 60)
                            | (assemblies[left_index].id.0 << 16)
                            | 0x5000
                            | left_edge_index as u64,
                    ),
                    ResolvedItemId(
                        (0x8_u64 << 60)
                            | (assemblies[right_index].id.0 << 16)
                            | 0x5000
                            | right_edge_index as u64,
                    ),
                ];
                geometry
                    .solids
                    .retain(|solid| !weather_ids.contains(&solid.id));
                removals.push((right_index, right_edge_index));
            }
        }
    }
    removals.sort_unstable();
    removals.dedup();
    for (assembly_index, edge_index) in removals.into_iter().rev() {
        assemblies[assembly_index].edges.remove(edge_index);
    }
}

fn resolve_one_roof(
    id: RoofAssemblyId,
    owner: GeometryOwnerId,
    roof: RoofPiece,
    source_piece_index: Option<usize>,
    source_tower_index: Option<usize>,
    parent: Option<RoofAssemblyId>,
    phase: RoofPhase,
    shed_high_side: Option<Direction>,
    support_post_parent: Option<&RoofAssembly>,
    walls: &[crate::WallAssembly],
    geometry: &mut ResolvedGeometry,
) -> RoofAssembly {
    // Project gates: 0.13 m positive build-up and 15–75 degree pitch are
    // animation/rendering constraints, not universal historic dimensions.
    let thickness = if roof.kind == RoofKind::Flat {
        0.18
    } else {
        0.13
    };
    let mut apse_walls = walls
        .iter()
        .filter(|wall| matches!(wall.source, crate::WallSourceId::ChurchApse { .. }))
        .collect::<Vec<_>>();
    apse_walls.sort_by_key(|wall| match wall.source {
        crate::WallSourceId::ChurchApse { facet } => facet,
        _ => unreachable!(),
    });
    let is_church_apse = source_piece_index == Some(4) && apse_walls.len() == 5;
    let apse_outline: Option<Vec<Vec2>> = is_church_apse.then(|| {
        let first = apse_walls[0];
        let mut points = vec![first.frame.origin - first.frame.tangent * first.length_metres * 0.5];
        points.extend(
            apse_walls
                .iter()
                .map(|wall| wall.frame.origin + wall.frame.tangent * wall.length_metres * 0.5),
        );
        let diameter_mid = (points[0] + points[points.len() - 1]) * 0.5;
        points
            .into_iter()
            .map(|point| {
                // The chord wall is 0.90 m thick; a 0.75 m radial eave keeps
                // the physical gutter outside the masonry even at the acute
                // five-sided shoulders.  This is a frozen coarse-detail gate,
                // not a universal historic apse overhang.
                point + (point - diameter_mid).normalize_or_zero() * roof.eave_metres.max(0.75)
            })
            .collect()
    });
    let polygons = if let Some(outline) = &apse_outline {
        let diameter_mid = (outline[0] + outline[outline.len() - 1]) * 0.5;
        let radius = outline
            .iter()
            .map(|point| point.distance(diameter_mid))
            .fold(0.0_f32, f32::max);
        let apex = Vec3::new(
            diameter_mid.x,
            roof.base_height_metres + radius * roof.pitch_degrees.to_radians().tan(),
            diameter_mid.y,
        );
        outline
            .windows(2)
            .map(|pair| {
                vec![
                    Vec3::new(pair[0].x, roof.base_height_metres, pair[0].y),
                    Vec3::new(pair[1].x, roof.base_height_metres, pair[1].y),
                    apex,
                ]
            })
            .collect::<Vec<_>>()
    } else {
        roof_face_polygons(roof, shed_high_side)
    };
    let node_base = StructuralNodeId((0xA_u64 << 60) | (id.0 << 8));
    let mut host_nodes = walls
        .iter()
        .filter(|wall| wall.replaced_by_owner.is_none())
        .filter(|wall| {
            let top = wall.base_elevation_metres + wall.height_metres;
            (top - roof.base_height_metres).abs() <= 0.35
                || (wall.base_elevation_metres <= roof.base_height_metres
                    && top >= roof.base_height_metres)
        })
        .map(|wall| wall.support_node)
        .collect::<Vec<_>>();
    host_nodes.sort_unstable();
    host_nodes.dedup();
    if host_nodes.is_empty() {
        host_nodes.extend(
            walls
                .iter()
                .filter(|wall| wall.replaced_by_owner.is_none())
                .filter(|wall| {
                    wall.base_elevation_metres + wall.height_metres
                        <= roof.base_height_metres + 0.05
                })
                .max_by(|left, right| {
                    (left.base_elevation_metres + left.height_metres)
                        .total_cmp(&(right.base_elevation_metres + right.height_metres))
                })
                .map(|wall| wall.support_node),
        );
    }
    let host_top = walls
        .iter()
        .filter(|wall| host_nodes.contains(&wall.support_node))
        .map(|wall| wall.base_elevation_metres + wall.height_metres)
        .fold(f32::NEG_INFINITY, f32::max)
        .min(roof.base_height_metres);
    let line_x = Vec3::new(roof.size.x * 0.5, 0.04, 0.12);
    let line_z = Vec3::new(0.12, 0.04, roof.size.y * 0.5);
    let mut plate_specs = match roof.ridge_axis {
        RidgeAxis::Z => vec![
            (
                Vec3::new(
                    roof.centre.x - roof.size.x * 0.5,
                    roof.base_height_metres,
                    roof.centre.y,
                ),
                line_z,
            ),
            (
                Vec3::new(
                    roof.centre.x + roof.size.x * 0.5,
                    roof.base_height_metres,
                    roof.centre.y,
                ),
                line_z,
            ),
        ],
        RidgeAxis::X => vec![
            (
                Vec3::new(
                    roof.centre.x,
                    roof.base_height_metres,
                    roof.centre.y - roof.size.y * 0.5,
                ),
                line_x,
            ),
            (
                Vec3::new(
                    roof.centre.x,
                    roof.base_height_metres,
                    roof.centre.y + roof.size.y * 0.5,
                ),
                line_x,
            ),
        ],
    };
    if is_church_apse {
        plate_specs = apse_walls
            .iter()
            .map(|wall| {
                (
                    Vec3::new(
                        wall.frame.origin.x,
                        roof.base_height_metres,
                        wall.frame.origin.y,
                    ),
                    Vec3::splat(0.12),
                )
            })
            .collect();
    } else if matches!(
        roof.kind,
        RoofKind::Hip | RoofKind::HalfHip | RoofKind::Pavilion
    ) {
        plate_specs = vec![
            (
                Vec3::new(
                    roof.centre.x - roof.size.x * 0.5,
                    roof.base_height_metres,
                    roof.centre.y,
                ),
                line_z,
            ),
            (
                Vec3::new(
                    roof.centre.x + roof.size.x * 0.5,
                    roof.base_height_metres,
                    roof.centre.y,
                ),
                line_z,
            ),
            (
                Vec3::new(
                    roof.centre.x,
                    roof.base_height_metres,
                    roof.centre.y - roof.size.y * 0.5,
                ),
                line_x,
            ),
            (
                Vec3::new(
                    roof.centre.x,
                    roof.base_height_metres,
                    roof.centre.y + roof.size.y * 0.5,
                ),
                line_x,
            ),
        ];
    } else if roof.kind == RoofKind::Conical {
        plate_specs = (0..8)
            .map(|index| {
                // Keep ring bearings between the 24 sector drain vertices.
                let angle =
                    std::f32::consts::TAU * index as f32 / 8.0 + std::f32::consts::TAU / 48.0;
                (
                    Vec3::new(
                        roof.centre.x + angle.cos() * roof.size.x * 0.5,
                        roof.base_height_metres,
                        roof.centre.y + angle.sin() * roof.size.y * 0.5,
                    ),
                    Vec3::new(0.14, 0.04, 0.14),
                )
            })
            .collect();
    }
    let support_nodes = (0..plate_specs.len())
        .map(|index| StructuralNodeId(node_base.0 + index as u64))
        .collect::<Vec<_>>();
    for (index, (position, half)) in plate_specs.into_iter().enumerate() {
        let node = support_nodes[index];
        geometry.structural_nodes.push(StructuralNode {
            id: node,
            owner,
            kind: if source_tower_index.is_some() {
                StructuralNodeKind::RoofTowerRing
            } else {
                StructuralNodeKind::RoofWallPlate
            },
            position,
            supported_by: host_nodes.clone(),
            grounded: false,
        });
        geometry.support_interfaces.push(SupportInterface {
            id: ResolvedItemId((0x9_u64 << 60) | (id.0 << 8) | index as u64),
            owner,
            node,
            bounds: ResolvedBounds {
                min: position - half,
                max: position + half,
            },
        });
        let plate_plan = Vec2::new(position.x, position.z);
        let nearest_wall = walls
            .iter()
            .filter(|wall| wall.replaced_by_owner.is_none())
            .filter(|wall| {
                wall.base_elevation_metres <= position.y + 0.02
                    && wall.base_elevation_metres + wall.height_metres >= position.y - 0.02
            })
            .map(|wall| {
                let half_length = wall.length_metres * 0.5;
                let along = (plate_plan - wall.frame.origin)
                    .dot(wall.frame.tangent)
                    .clamp(-half_length, half_length);
                let contact = wall.frame.origin + wall.frame.tangent * along;
                (wall, contact, contact.distance(plate_plan))
            })
            .min_by(|left, right| left.2.total_cmp(&right.2));
        if let Some((wall, contact, distance)) = nearest_wall
            && distance > 0.65
        {
            let direction = (plate_plan - contact).normalize_or_zero();
            let beam_id = ResolvedItemId((0x8_u64 << 60) | (id.0 << 8) | 0x20 | index as u64);
            geometry.solids.push(ResolvedSolid {
                id: beam_id,
                owner,
                centre: Vec3::new(
                    (plate_plan.x + contact.x) * 0.5 + direction.x * 0.01,
                    position.y,
                    (plate_plan.y + contact.y) * 0.5 + direction.y * 0.01,
                ),
                size: Vec3::new((distance - 0.02).max(0.02), 0.18, 0.18),
                yaw_radians: direction.y.atan2(direction.x),
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::RoofFraming,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: vec![wall.support_node],
            });
            geometry.support_interfaces.push(SupportInterface {
                id: ResolvedItemId((0x9_u64 << 60) | (id.0 << 8) | 0x20 | index as u64),
                owner,
                node: wall.support_node,
                bounds: ResolvedBounds {
                    min: Vec3::new(contact.x - 0.09, position.y - 0.09, contact.y - 0.09),
                    max: Vec3::new(contact.x + 0.09, position.y + 0.09, contact.y + 0.09),
                },
            });
        }
        // A dormer may need a concealed support from the host wall to its curb,
        // but never a generic post continuing from that wall to the child
        // eave. The latter produced the two freestanding poles visible in the
        // parent's OpeningCut. Standalone roofs continue to use their eave as
        // the fallback top; child roofs receive the actual curb elevation.
        let support_top = support_post_parent
            .and_then(|parent| roof_surface_height_at(parent, plate_plan))
            .unwrap_or(roof.base_height_metres);
        if support_post_parent.is_none() && host_top.is_finite() && support_top - host_top > 0.35 {
            let height = support_top - host_top;
            let post_id = ResolvedItemId((0x8_u64 << 60) | (id.0 << 8) | index as u64);
            let host_node = host_nodes[0];
            geometry.solids.push(ResolvedSolid {
                id: post_id,
                owner,
                centre: Vec3::new(position.x, host_top + height * 0.5, position.z),
                size: Vec3::new(0.22, height, 0.22),
                yaw_radians: 0.0,
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::RoofFraming,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: vec![host_node],
            });
            geometry.support_interfaces.push(SupportInterface {
                id: ResolvedItemId((0x9_u64 << 60) | (id.0 << 8) | 0x40 | index as u64),
                owner,
                node: host_node,
                bounds: ResolvedBounds {
                    min: Vec3::new(position.x - 0.11, host_top - 0.01, position.z - 0.11),
                    max: Vec3::new(position.x + 0.11, host_top + 0.01, position.z + 0.11),
                },
            });
        }
    }
    if is_church_apse {
        // The polygonal apse uses a continuous timber wall plate between the
        // 11.35 m masonry chord tops and the 11.50 m roof planes.  Besides a
        // credible bearing chain this keeps the eave gutter outside masonry
        // rather than intersecting the opening spandrels at acute corners.
        for (index, wall) in apse_walls.iter().enumerate() {
            let plate_id = ResolvedItemId((0x8_u64 << 60) | (id.0 << 8) | 0x80 | index as u64);
            geometry.solids.push(ResolvedSolid {
                id: plate_id,
                owner,
                centre: Vec3::new(wall.frame.origin.x, 11.425, wall.frame.origin.y),
                size: Vec3::new(wall.length_metres, 0.15, 0.80),
                yaw_radians: -wall.frame.tangent.y.atan2(wall.frame.tangent.x),
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::RoofFraming,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: vec![wall.support_node],
            });
            geometry.support_interfaces.push(SupportInterface {
                id: ResolvedItemId((0x9_u64 << 60) | (id.0 << 8) | 0x80 | index as u64),
                owner,
                node: wall.support_node,
                bounds: ResolvedBounds {
                    min: Vec3::new(
                        wall.frame.origin.x - 0.08,
                        11.335,
                        wall.frame.origin.y - 0.08,
                    ),
                    max: Vec3::new(
                        wall.frame.origin.x + 0.08,
                        11.365,
                        wall.frame.origin.y + 0.08,
                    ),
                },
            });
        }
    }
    let mut faces = Vec::new();
    for (index, polygon) in polygons.iter().enumerate() {
        let face_id = ResolvedItemId((0xA_u64 << 60) | (id.0 << 16) | index as u64);
        let catchment_id = ResolvedItemId((0xC_u64 << 60) | (id.0 << 16) | index as u64);
        let route_id = ResolvedItemId((0xD_u64 << 60) | (id.0 << 16) | index as u64);
        let outlet_id = ResolvedItemId((0xE_u64 << 60) | (id.0 << 16) | index as u64);
        let bounds = roof_polygon_bounds(polygon);
        let low = polygon
            .iter()
            .min_by(|a, b| a.y.total_cmp(&b.y))
            .copied()
            .unwrap();
        let centre = polygon.iter().copied().sum::<Vec3>() / polygon.len() as f32;
        geometry.surfaces.push(ResolvedSurface {
            id: catchment_id,
            owner,
            bounds,
            role: SurfaceRole::RoofDrainage,
            shape: crate::ResolvedSurfaceShape::Planar,
        });
        geometry.voids.push(ResolvedVoid {
            id: outlet_id,
            owner,
            bounds: ResolvedBounds {
                min: low - Vec3::splat(0.04),
                max: low + Vec3::splat(0.04),
            },
            role: VoidRole::Drain,
            shape: crate::ResolvedVoidShape::Box,
            subtracts_from: owner,
        });
        geometry.drainage_routes.push(DrainageRoute {
            id: route_id,
            owner,
            outlet_void: outlet_id,
            inlet: centre,
            outlet: low,
        });
        geometry.drainage_catchments.push(DrainageCatchment {
            id: catchment_id,
            owner,
            walk_solid: face_id,
            toe_channel_solids: Vec::new(),
            drainage_surface: catchment_id,
            outlet_route: route_id,
            centre,
            tangent: Vec2::X,
            outward: Vec2::new(low.x - centre.x, low.z - centre.z).normalize_or_zero(),
            length_metres: (bounds.max.x - bounds.min.x).max(bounds.max.z - bounds.min.z),
            width_metres: (bounds.max.x - bounds.min.x).min(bounds.max.z - bounds.min.z),
            inner_elevation_metres: polygon
                .iter()
                .map(|p| p.y)
                .fold(f32::NEG_INFINITY, f32::max),
            outer_elevation_metres: low.y,
            outlet_along_metres: 0.0,
        });
        faces.push(RoofFace {
            id: face_id,
            polygon: polygon.to_vec(),
            cutouts: Vec::new(),
            plane: roof_plane(polygon),
            pitch_degrees: roof.pitch_degrees,
            thickness_metres: thickness,
            material: RoofMaterial::ClayTile,
            support_nodes: support_nodes.clone(),
            drainage_catchment: catchment_id,
        });
    }
    let mut edges: Vec<RoofEdge> = Vec::new();
    for face in &faces {
        for index in 0..face.polygon.len() {
            let a = face.polygon[index];
            let b = face.polygon[(index + 1) % face.polygon.len()];
            if let Some(edge) = edges.iter_mut().find(|edge| {
                (same_roof_vertex(edge.start, a) && same_roof_vertex(edge.end, b))
                    || (same_roof_vertex(edge.start, b) && same_roof_vertex(edge.end, a))
            }) {
                edge.adjacent_faces.push(face.id);
            } else {
                let edge_id = ResolvedItemId((0xB_u64 << 60) | (id.0 << 16) | edges.len() as u64);
                edges.push(RoofEdge {
                    id: edge_id,
                    start: a,
                    end: b,
                    kind: RoofEdgeKind::Eave,
                    adjacent_faces: vec![face.id],
                    flashing: None,
                    drainage_terminal: None,
                });
            }
        }
    }
    for (edge_index, edge) in edges.iter_mut().enumerate() {
        if edge.adjacent_faces.len() == 2 {
            edge.kind = if (edge.start.y - edge.end.y).abs() <= 0.01 {
                RoofEdgeKind::Ridge
            } else {
                RoofEdgeKind::Hip
            };
        } else if (edge.start.y - edge.end.y).abs() <= 0.01
            && (((edge.start.y - roof.base_height_metres).abs() <= 0.01
                && (edge.end.y - roof.base_height_metres).abs() <= 0.01)
                || roof.kind == RoofKind::HalfHip)
        {
            edge.kind = RoofEdgeKind::Eave;
            edge.drainage_terminal = faces
                .iter()
                .find(|face| {
                    face.polygon
                        .iter()
                        .any(|p| same_roof_vertex(*p, edge.start))
                        && face.polygon.iter().any(|p| same_roof_vertex(*p, edge.end))
                })
                .and_then(|face| {
                    geometry
                        .drainage_catchments
                        .iter()
                        .find(|catchment| catchment.id == face.drainage_catchment)
                })
                .and_then(|catchment| {
                    geometry
                        .drainage_routes
                        .iter()
                        .find(|route| route.id == catchment.outlet_route)
                })
                .map(|route| route.outlet_void);
        } else if roof.kind == RoofKind::Shed && (edge.start.y - edge.end.y).abs() <= 0.01 {
            edge.kind = RoofEdgeKind::WallAbutment;
            let flashing_id =
                ResolvedItemId((0x8_u64 << 60) | (id.0 << 16) | 0x5800 | edge_index as u64);
            edge.flashing = Some(flashing_id);
            let delta = edge.end - edge.start;
            geometry.solids.push(ResolvedSolid {
                id: flashing_id,
                owner,
                centre: (edge.start + edge.end) * 0.5 + Vec3::Y * 0.035,
                size: Vec3::new(Vec2::new(delta.x, delta.z).length(), 0.07, 0.18),
                yaw_radians: delta.z.atan2(delta.x),
                crossfall_radians: 0.12,
                longfall_radians: 0.0,
                role: SolidRole::RoofFlashing,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: support_nodes.clone(),
            });
            geometry.support_interfaces.push(SupportInterface {
                id: ResolvedItemId((0x9_u64 << 60) | (id.0 << 16) | 0x5800 | edge_index as u64),
                owner,
                node: support_nodes[0],
                bounds: ResolvedBounds {
                    min: (edge.start + edge.end) * 0.5 - Vec3::new(0.08, 0.025, 0.08),
                    max: (edge.start + edge.end) * 0.5 + Vec3::new(0.08, 0.025, 0.08),
                },
            });
        } else {
            edge.kind = RoofEdgeKind::GableVerge;
        }
        if matches!(
            edge.kind,
            RoofEdgeKind::Eave | RoofEdgeKind::Ridge | RoofEdgeKind::Hip | RoofEdgeKind::GableVerge
        ) {
            let weather_id =
                ResolvedItemId((0x8_u64 << 60) | (id.0 << 16) | 0x5000 | edge_index as u64);
            let delta = edge.end - edge.start;
            let plan_length = Vec2::new(delta.x, delta.z).length().max(0.05);
            let centre = (edge.start + edge.end) * 0.5;
            // Roof faces may drain to either end of a shared perimeter eave.
            // Leave a physical outlet gap at both corners rather than letting
            // the gutter cuboid seal an adjacent face's terminal.
            let treated_plan_length = if edge.kind == RoofEdgeKind::Eave {
                (plan_length - 0.36_f32.min(plan_length * 0.5)).max(0.05)
            } else {
                plan_length
            };
            let edge_pitch = delta.y.atan2(plan_length);
            let treated_length = if edge.kind == RoofEdgeKind::Eave {
                treated_plan_length
            } else {
                // Edge treatments are authoritative solids on the actual 3D
                // edge, not horizontal plan-projection bars.
                treated_plan_length / edge_pitch.cos().abs().max(0.01)
            };
            geometry.solids.push(ResolvedSolid {
                id: weather_id,
                owner,
                centre: centre
                    + if edge.kind == RoofEdgeKind::Eave {
                        Vec3::NEG_Y * 0.06
                    } else {
                        Vec3::Y * 0.035
                    },
                size: Vec3::new(
                    treated_length,
                    if edge.kind == RoofEdgeKind::Eave {
                        0.12
                    } else {
                        0.07
                    },
                    if edge.kind == RoofEdgeKind::Eave {
                        0.16
                    } else {
                        0.14
                    },
                ),
                yaw_radians: delta.z.atan2(delta.x),
                // Edge treatment's long axis is the typed source contour.
                // Applying coping crossfall as an X rotation skewed that axis
                // off the roof plane and produced detached diagonal rods.
                crossfall_radians: 0.0,
                longfall_radians: if edge.kind == RoofEdgeKind::Eave {
                    0.012
                } else {
                    edge_pitch
                },
                role: if edge.kind == RoofEdgeKind::Eave {
                    SolidRole::RoofGutter
                } else {
                    SolidRole::RoofEdgeTreatment
                },
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: support_nodes.clone(),
            });
            geometry.support_interfaces.push(SupportInterface {
                id: ResolvedItemId((0x9_u64 << 60) | (id.0 << 16) | 0x5000 | edge_index as u64),
                owner,
                node: support_nodes[0],
                bounds: ResolvedBounds {
                    min: centre - Vec3::new(0.08, 0.025, 0.08),
                    max: centre + Vec3::new(0.08, 0.025, 0.08),
                },
            });
        }
    }
    let hx = roof.size.x * 0.5;
    let hz = roof.size.y * 0.5;
    let roof_grid_point = |position: Vec2| {
        GridPoint::new(
            (position.x / GRID_UNIT_METRES).round() as i32,
            (position.y / GRID_UNIT_METRES).round() as i32,
        )
    };
    let infill_material = if walls
        .iter()
        .any(|wall| wall.material == crate::WallMaterialClass::TimberInfill)
    {
        RoofMaterial::TimberInfill
    } else {
        RoofMaterial::MasonryInfill
    };
    let mut enclosure_faces = Vec::new();
    if roof.kind == RoofKind::Gable {
        let apex_y = faces
            .iter()
            .flat_map(|face| &face.polygon)
            .map(|point| point.y)
            .fold(roof.base_height_metres, f32::max);
        let (first, second) = match roof.ridge_axis {
            RidgeAxis::Z => {
                let triangle = |z: f32, reverse: bool| {
                    let mut polygon = vec![
                        Vec3::new(
                            roof.centre.x - roof.size.x * 0.5,
                            roof.base_height_metres,
                            z,
                        ),
                        Vec3::new(roof.centre.x, apex_y, z),
                        Vec3::new(
                            roof.centre.x + roof.size.x * 0.5,
                            roof.base_height_metres,
                            z,
                        ),
                    ];
                    if reverse {
                        polygon.reverse();
                    }
                    polygon
                };
                (
                    triangle(roof.centre.y - roof.size.y * 0.5, false),
                    triangle(roof.centre.y + roof.size.y * 0.5, true),
                )
            }
            RidgeAxis::X => {
                let triangle = |x: f32, reverse: bool| {
                    let mut polygon = vec![
                        Vec3::new(
                            x,
                            roof.base_height_metres,
                            roof.centre.y - roof.size.y * 0.5,
                        ),
                        Vec3::new(x, apex_y, roof.centre.y),
                        Vec3::new(
                            x,
                            roof.base_height_metres,
                            roof.centre.y + roof.size.y * 0.5,
                        ),
                    ];
                    if reverse {
                        polygon.reverse();
                    }
                    polygon
                };
                (
                    triangle(roof.centre.x - roof.size.x * 0.5, true),
                    triangle(roof.centre.x + roof.size.x * 0.5, false),
                )
            }
        };
        for (index, polygon) in [first, second].into_iter().enumerate() {
            enclosure_faces.push(RoofEnclosureFace {
                id: ResolvedItemId((0xA_u64 << 60) | (id.0 << 16) | 0x4000 | index as u64),
                polygon,
                material: infill_material,
                support_nodes: support_nodes.clone(),
            });
        }
    }
    if roof.kind == RoofKind::HalfHip {
        let face_hx = roof.size.x * 0.5 + roof.eave_metres;
        let face_hz = roof.size.y * 0.5 + roof.eave_metres;
        let shoulder_fraction = 0.55;
        let polygons = match roof.ridge_axis {
            RidgeAxis::Z => {
                let shoulder_x = face_hx * (1.0 - shoulder_fraction);
                let shoulder_y = roof.base_height_metres
                    + face_hx * roof.pitch_degrees.to_radians().tan() * shoulder_fraction;
                let gable = |z: f32, reverse: bool| {
                    let mut polygon = vec![
                        Vec3::new(roof.centre.x - face_hx, roof.base_height_metres, z),
                        Vec3::new(roof.centre.x + face_hx, roof.base_height_metres, z),
                        Vec3::new(roof.centre.x + shoulder_x, shoulder_y, z),
                        Vec3::new(roof.centre.x - shoulder_x, shoulder_y, z),
                    ];
                    if reverse {
                        polygon.reverse();
                    }
                    polygon
                };
                vec![
                    gable(roof.centre.y - face_hz, false),
                    gable(roof.centre.y + face_hz, true),
                ]
            }
            RidgeAxis::X => {
                let shoulder_z = face_hz * (1.0 - shoulder_fraction);
                let shoulder_y = roof.base_height_metres
                    + face_hz * roof.pitch_degrees.to_radians().tan() * shoulder_fraction;
                let gable = |x: f32, reverse: bool| {
                    let mut polygon = vec![
                        Vec3::new(x, roof.base_height_metres, roof.centre.y - face_hz),
                        Vec3::new(x, roof.base_height_metres, roof.centre.y + face_hz),
                        Vec3::new(x, shoulder_y, roof.centre.y + shoulder_z),
                        Vec3::new(x, shoulder_y, roof.centre.y - shoulder_z),
                    ];
                    if reverse {
                        polygon.reverse();
                    }
                    polygon
                };
                vec![
                    gable(roof.centre.x - face_hx, true),
                    gable(roof.centre.x + face_hx, false),
                ]
            }
        };
        for (index, polygon) in polygons.into_iter().enumerate() {
            enclosure_faces.push(RoofEnclosureFace {
                id: ResolvedItemId((0xA_u64 << 60) | (id.0 << 16) | 0x4200 | index as u64),
                polygon,
                material: infill_material,
                support_nodes: support_nodes.clone(),
            });
        }
    }
    // A raised primary roof needs an actual clerestory/attic wall under each
    // eave; posts alone are a support skeleton, not a weather-tight building
    // envelope.  This is especially important for the cathedral nave above
    // its independent aisle roofs.
    if parent.is_none()
        && host_top.is_finite()
        && roof.base_height_metres - host_top > 0.35
        && matches!(roof.kind, RoofKind::Gable | RoofKind::Shed)
    {
        let (first, second) = match roof.ridge_axis {
            RidgeAxis::Z => {
                let wall = |x: f32, reverse: bool| {
                    let mut polygon = vec![
                        Vec3::new(x, host_top, roof.centre.y - hz),
                        Vec3::new(x, host_top, roof.centre.y + hz),
                        Vec3::new(x, roof.base_height_metres, roof.centre.y + hz),
                        Vec3::new(x, roof.base_height_metres, roof.centre.y - hz),
                    ];
                    if reverse {
                        polygon.reverse();
                    }
                    polygon
                };
                (
                    wall(roof.centre.x - hx, true),
                    wall(roof.centre.x + hx, false),
                )
            }
            RidgeAxis::X => {
                let wall = |z: f32, reverse: bool| {
                    let mut polygon = vec![
                        Vec3::new(roof.centre.x - hx, host_top, z),
                        Vec3::new(roof.centre.x + hx, host_top, z),
                        Vec3::new(roof.centre.x + hx, roof.base_height_metres, z),
                        Vec3::new(roof.centre.x - hx, roof.base_height_metres, z),
                    ];
                    if reverse {
                        polygon.reverse();
                    }
                    polygon
                };
                (
                    wall(roof.centre.y - hz, false),
                    wall(roof.centre.y + hz, true),
                )
            }
        };
        for (slot, polygon) in [first, second].into_iter().enumerate() {
            enclosure_faces.push(RoofEnclosureFace {
                id: ResolvedItemId((0xA_u64 << 60) | (id.0 << 16) | 0x4300 | slot as u64),
                polygon,
                material: infill_material,
                support_nodes: support_nodes.clone(),
            });
        }
    }
    RoofAssembly {
        id,
        owner,
        kind: roof.kind,
        outer_loop: RoofFootprintLoop {
            vertices: apse_outline.map_or_else(
                || {
                    vec![
                        roof_grid_point(roof.centre + Vec2::new(-hx, -hz)),
                        roof_grid_point(roof.centre + Vec2::new(hx, -hz)),
                        roof_grid_point(roof.centre + Vec2::new(hx, hz)),
                        roof_grid_point(roof.centre + Vec2::new(-hx, hz)),
                    ]
                },
                |outline| outline.into_iter().map(roof_grid_point).collect(),
            ),
        },
        holes: Vec::new(),
        faces,
        enclosure_faces,
        edges,
        children: Vec::new(),
        abutments: Vec::new(),
        parent,
        material: RoofMaterial::ClayTile,
        phase,
        pivot_policy: RoofPivotPolicy::KeepEave,
        shed_high_side,
        support_nodes,
        source_piece_index,
        source_tower_index,
    }
}

fn resolve_roof_assemblies(
    program: &BuildingProgram,
    roofs: &[RoofPiece],
    dormers: &[RoofDormer],
    towers: &[RoundTower],
    square_towers: &[SquareTower],
    stairs: &[Stair],
    walls: &[crate::WallAssembly],
    openings: &[crate::OpeningAssembly],
    geometry: &mut ResolvedGeometry,
) -> Vec<RoofAssembly> {
    let mut assemblies = Vec::new();
    for (index, roof) in roofs.iter().copied().enumerate() {
        let id = RoofAssemblyId(index as u64 + 1);
        let shed_high_side = match (program.archetype, index, roof.kind) {
            (BuildingArchetype::Cathedral, 1, RoofKind::Shed) => Some(Direction::North),
            (BuildingArchetype::Cathedral, 2, RoofKind::Shed) => Some(Direction::South),
            (_, _, RoofKind::Shed) => Some(match roof.ridge_axis {
                RidgeAxis::Z => Direction::East,
                RidgeAxis::X => Direction::North,
            }),
            _ => None,
        };
        assemblies.push(resolve_one_roof(
            id,
            GeometryOwnerId(60_000 + index as u32),
            roof,
            Some(index),
            None,
            None,
            RoofPhase::Primary,
            shed_high_side,
            None,
            walls,
            geometry,
        ));
    }
    if let Some(parent) = assemblies
        .first()
        .map(|assembly| (assembly.id, assembly.owner))
    {
        for index in 1..roofs.len() {
            let child_recipe = roofs[index];
            let parent_recipe = roofs[0];
            let child_min = child_recipe.centre - child_recipe.size * 0.5;
            let child_max = child_recipe.centre + child_recipe.size * 0.5;
            let parent_min = parent_recipe.centre - parent_recipe.size * 0.5;
            let parent_max = parent_recipe.centre + parent_recipe.size * 0.5;
            let overlaps = child_min.x < parent_max.x
                && child_max.x > parent_min.x
                && child_min.y < parent_max.y
                && child_max.y > parent_min.y;
            if overlaps && child_recipe.base_height_metres > parent_recipe.base_height_metres + 0.5
            {
                let child_id = assemblies[index].id;
                assemblies[index].parent = Some(parent.0);
                assemblies[index].phase = RoofPhase::AttachedChild;
                let enclosure_material = if walls
                    .iter()
                    .any(|wall| wall.material == crate::WallMaterialClass::TimberInfill)
                {
                    RoofMaterial::TimberInfill
                } else {
                    RoofMaterial::MasonryInfill
                };
                let enclosure_supports = assemblies[index].support_nodes.clone();
                let top = child_recipe.base_height_metres;
                let parent_snapshot = assemblies[0].clone();
                let parent_height = |point: Vec2| {
                    roof_surface_height_at(&parent_snapshot, point)
                        .unwrap_or(parent_recipe.base_height_metres)
                };
                for (slot, polygon) in [
                    vec![
                        Vec3::new(
                            child_min.x,
                            parent_height(Vec2::new(child_min.x, child_min.y)),
                            child_min.y,
                        ),
                        Vec3::new(
                            child_max.x,
                            parent_height(Vec2::new(child_max.x, child_min.y)),
                            child_min.y,
                        ),
                        Vec3::new(child_max.x, top, child_min.y),
                        Vec3::new(child_min.x, top, child_min.y),
                    ],
                    vec![
                        Vec3::new(
                            child_max.x,
                            parent_height(Vec2::new(child_max.x, child_max.y)),
                            child_max.y,
                        ),
                        Vec3::new(
                            child_min.x,
                            parent_height(Vec2::new(child_min.x, child_max.y)),
                            child_max.y,
                        ),
                        Vec3::new(child_min.x, top, child_max.y),
                        Vec3::new(child_max.x, top, child_max.y),
                    ],
                    vec![
                        Vec3::new(
                            child_min.x,
                            parent_height(Vec2::new(child_min.x, child_max.y)),
                            child_max.y,
                        ),
                        Vec3::new(
                            child_min.x,
                            parent_height(Vec2::new(child_min.x, child_min.y)),
                            child_min.y,
                        ),
                        Vec3::new(child_min.x, top, child_min.y),
                        Vec3::new(child_min.x, top, child_max.y),
                    ],
                    vec![
                        Vec3::new(
                            child_max.x,
                            parent_height(Vec2::new(child_max.x, child_min.y)),
                            child_min.y,
                        ),
                        Vec3::new(
                            child_max.x,
                            parent_height(Vec2::new(child_max.x, child_max.y)),
                            child_max.y,
                        ),
                        Vec3::new(child_max.x, top, child_max.y),
                        Vec3::new(child_max.x, top, child_min.y),
                    ],
                ]
                .into_iter()
                .enumerate()
                {
                    assemblies[index].enclosure_faces.push(RoofEnclosureFace {
                        id: ResolvedItemId(
                            (0xA_u64 << 60) | (child_id.0 << 16) | 0x4200 | slot as u64,
                        ),
                        polygon,
                        material: enclosure_material,
                        support_nodes: enclosure_supports.clone(),
                    });
                }
                let cut_id = ResolvedItemId((0xF_u64 << 60) | child_id.0);
                let bounds = ResolvedBounds {
                    min: Vec3::new(
                        child_min.x,
                        parent_recipe.base_height_metres - 0.2,
                        child_min.y,
                    ),
                    max: Vec3::new(
                        child_max.x,
                        child_recipe.base_height_metres + 5.0,
                        child_max.y,
                    ),
                };
                geometry.voids.push(ResolvedVoid {
                    id: cut_id,
                    owner: parent.1,
                    bounds,
                    role: VoidRole::RoofOpening,
                    shape: crate::ResolvedVoidShape::Box,
                    subtracts_from: parent.1,
                });
                let child_supports = assemblies[index].support_nodes.clone();
                let child_copy = assemblies[index].clone();
                let cut_edges =
                    cut_parent_roof_face(&mut assemblies[0], &child_copy, bounds, geometry);
                let valleys =
                    bind_child_valleys(&mut assemblies[0], &child_copy, &cut_edges, geometry);
                let flashing_ids = assemblies[0]
                    .edges
                    .iter()
                    .filter(|edge| cut_edges.contains(&edge.id))
                    .filter_map(|edge| edge.flashing)
                    .collect();
                assemblies[0].children.push(RoofChildAssembly {
                    child: child_id,
                    kind: RoofChildKind::CrossGable,
                    parent_cut: cut_id,
                    trimmer_nodes: child_supports,
                    valley_edges: valleys,
                    flashing_ids,
                    facade_wall: None,
                    split_eave_edges: Vec::new(),
                });
            }
        }
    }
    let parent = assemblies.first().map(|roof| roof.id);
    for (index, dormer) in dormers.iter().copied().enumerate() {
        let scale = if dormer.kind == DormerKind::TransverseGable {
            2.20
        } else {
            1.0
        };
        let inward = match dormer.facing {
            Direction::North => -Vec2::Y,
            Direction::South => Vec2::Y,
            Direction::East => -Vec2::X,
            Direction::West => Vec2::X,
        };
        let ridge_axis = if matches!(dormer.facing, Direction::North | Direction::South) {
            RidgeAxis::Z
        } else {
            RidgeAxis::X
        };
        let top = dormer.base_height_metres
            + dormer.height_metres
                * if dormer.kind == DormerKind::TransverseGable {
                    1.35
                } else {
                    1.0
                };
        let tangent = if matches!(dormer.facing, Direction::North | Direction::South) {
            Vec2::X
        } else {
            Vec2::Y
        };
        let half_width = dormer.width_metres * scale * 0.5;
        let roof_eave = if dormer.kind == DormerKind::TransverseGable {
            0.16
        } else {
            0.10
        };
        let fallback_depth = dormer.depth_metres * 0.84;
        let minimum_usable_depth = fallback_depth.min(0.75);
        // The rear edge of a dormer is not a second free gable. Extend the
        // child inward until its eave plane meets the actual parent weather
        // plane at both cheeks. The small overhang then seats on that seam.
        let enclosure_depth = assemblies
            .first()
            .and_then(|parent| {
                (0..=800)
                    .map(|step| minimum_usable_depth + roof_eave + step as f32 * 0.01)
                    .find(|depth| {
                        [-1.0_f32, 1.0].into_iter().all(|side| {
                            let point = dormer.centre
                                + inward * *depth
                                + tangent * side * (half_width + roof_eave);
                            roof_surface_height_at(parent, point)
                                .is_some_and(|height| height >= top - 0.015)
                        })
                    })
            })
            .map(|rear_edge_depth| (rear_edge_depth - roof_eave).max(minimum_usable_depth))
            .unwrap_or(fallback_depth);
        let size = if ridge_axis == RidgeAxis::Z {
            Vec2::new(dormer.width_metres * scale, enclosure_depth)
        } else {
            Vec2::new(enclosure_depth, dormer.width_metres * scale)
        };
        let recipe = RoofPiece {
            kind: if dormer.kind == DormerKind::Shed {
                RoofKind::Shed
            } else {
                RoofKind::Gable
            },
            centre: dormer.centre + inward * enclosure_depth * 0.5,
            size,
            base_height_metres: top,
            pitch_degrees: 48.0,
            ridge_axis,
            eave_metres: roof_eave,
            gable_profile: dormer.gable_profile,
        };
        let id = RoofAssemblyId(1_000 + index as u64);
        let mut child = resolve_one_roof(
            id,
            GeometryOwnerId(61_000 + index as u32),
            recipe,
            None,
            None,
            parent,
            RoofPhase::AttachedChild,
            (recipe.kind == RoofKind::Shed).then_some(dormer.facing.opposite()),
            assemblies.first(),
            walls,
            geometry,
        );
        if recipe.kind == RoofKind::Gable {
            // `resolve_one_roof` normally closes both gable ends. A dormer
            // owns only the visible front gable; its rear terminates in the
            // parent weather plane. Remove the otherwise floating rear
            // triangle and its two verge caps. The parent's cut-edge flashing
            // owns the seated head joint.
            child.enclosure_faces.retain(|face| {
                let mean_depth = face
                    .polygon
                    .iter()
                    .map(|point| (Vec2::new(point.x, point.z) - dormer.centre).dot(-inward))
                    .sum::<f32>()
                    / face.polygon.len() as f32;
                mean_depth > -enclosure_depth + 0.02
            });
            let rear_edge_depth = -(enclosure_depth + roof_eave);
            for (edge_index, edge) in child.edges.iter_mut().enumerate() {
                let start_depth =
                    (Vec2::new(edge.start.x, edge.start.z) - dormer.centre).dot(-inward);
                let end_depth = (Vec2::new(edge.end.x, edge.end.z) - dormer.centre).dot(-inward);
                if edge.kind == RoofEdgeKind::GableVerge
                    && (start_depth - rear_edge_depth).abs() <= 0.02
                    && (end_depth - rear_edge_depth).abs() <= 0.02
                {
                    edge.kind = RoofEdgeKind::OpeningCut;
                    let weather_id =
                        ResolvedItemId((0x8_u64 << 60) | (id.0 << 16) | 0x5000 | edge_index as u64);
                    let interface_id =
                        ResolvedItemId((0x9_u64 << 60) | (id.0 << 16) | 0x5000 | edge_index as u64);
                    geometry.solids.retain(|solid| solid.id != weather_id);
                    geometry
                        .support_interfaces
                        .retain(|interface| interface.id != interface_id);
                }
            }
        }
        let front_left = dormer.centre - tangent * half_width;
        let front_right = dormer.centre + tangent * half_width;
        let rear_left = front_left + inward * enclosure_depth;
        let rear_right = front_right + inward * enclosure_depth;
        let parent_height = |point: Vec2| {
            assemblies
                .first()
                .and_then(|parent| roof_surface_height_at(parent, point))
                .unwrap_or(dormer.base_height_metres)
        };
        for (slot, polygon) in [
            vec![
                Vec3::new(front_left.x, parent_height(front_left), front_left.y),
                Vec3::new(front_right.x, parent_height(front_right), front_right.y),
                Vec3::new(front_right.x, top, front_right.y),
                Vec3::new(front_left.x, top, front_left.y),
            ],
            vec![
                Vec3::new(front_left.x, parent_height(front_left), front_left.y),
                Vec3::new(front_left.x, top, front_left.y),
                Vec3::new(rear_left.x, top, rear_left.y),
                Vec3::new(rear_left.x, parent_height(rear_left), rear_left.y),
            ],
            vec![
                Vec3::new(front_right.x, parent_height(front_right), front_right.y),
                Vec3::new(rear_right.x, parent_height(rear_right), rear_right.y),
                Vec3::new(rear_right.x, top, rear_right.y),
                Vec3::new(front_right.x, top, front_right.y),
            ],
        ]
        .into_iter()
        .enumerate()
        {
            child.enclosure_faces.push(RoofEnclosureFace {
                id: ResolvedItemId((0xA_u64 << 60) | (id.0 << 16) | 0x4100 | slot as u64),
                polygon,
                material: if walls
                    .iter()
                    .any(|wall| wall.material == crate::WallMaterialClass::TimberInfill)
                {
                    RoofMaterial::TimberInfill
                } else {
                    RoofMaterial::MasonryInfill
                },
                support_nodes: child.support_nodes.clone(),
            });
        }
        if parent.is_some() {
            let cut_id = ResolvedItemId((0xF_u64 << 60) | id.0);
            let bounds = ResolvedBounds {
                min: Vec3::new(
                    recipe.centre.x - recipe.size.x * 0.5,
                    recipe.base_height_metres - 0.2,
                    recipe.centre.y - recipe.size.y * 0.5,
                ),
                max: Vec3::new(
                    recipe.centre.x + recipe.size.x * 0.5,
                    recipe.base_height_metres + 4.0,
                    recipe.centre.y + recipe.size.y * 0.5,
                ),
            };
            geometry.voids.push(ResolvedVoid {
                id: cut_id,
                owner: assemblies[0].owner,
                bounds,
                role: VoidRole::RoofOpening,
                shape: crate::ResolvedVoidShape::Box,
                subtracts_from: assemblies[0].owner,
            });
            let child_kind = match dormer.kind {
                DormerKind::Gabled | DormerKind::Hipped => RoofChildKind::GabledDormer,
                DormerKind::Shed => RoofChildKind::ShedDormer,
                DormerKind::TransverseGable => RoofChildKind::CrossGable,
            };
            let cut_edges = cut_parent_roof_face(&mut assemblies[0], &child, bounds, geometry);
            let valleys = bind_child_valleys(&mut assemblies[0], &child, &cut_edges, geometry);
            let flashing_ids = assemblies[0]
                .edges
                .iter()
                .filter(|edge| cut_edges.contains(&edge.id))
                .filter_map(|edge| edge.flashing)
                .collect();
            assemblies[0].children.push(RoofChildAssembly {
                child: id,
                kind: child_kind,
                parent_cut: cut_id,
                trimmer_nodes: child.support_nodes.clone(),
                valley_edges: valleys,
                flashing_ids,
                facade_wall: None,
                split_eave_edges: Vec::new(),
            });
            if dormer.kind == DormerKind::TransverseGable {
                split_cross_gable_parent_eave(
                    &mut assemblies[0],
                    id,
                    dormer.centre,
                    tangent,
                    dormer.width_metres * scale,
                );
            }
        }
        child.phase = RoofPhase::AttachedChild;
        assemblies.push(child);
    }
    for (index, tower) in towers.iter().copied().enumerate() {
        if let Some(roof) = tower.roof {
            let id = RoofAssemblyId(2_000 + index as u64);
            assemblies.push(resolve_one_roof(
                id,
                GeometryOwnerId(62_000 + index as u32),
                roof,
                None,
                Some(index),
                None,
                RoofPhase::Primary,
                None,
                None,
                walls,
                geometry,
            ));
        }
    }
    for (index, tower) in square_towers.iter().copied().enumerate() {
        let id = RoofAssemblyId(3_000 + index as u64);
        assemblies.push(resolve_one_roof(
            id,
            GeometryOwnerId(63_000 + index as u32),
            tower.roof,
            None,
            Some(index),
            None,
            RoofPhase::Primary,
            None,
            None,
            walls,
            geometry,
        ));
    }
    // A tower piercing the principal roof is a true abutment, not two
    // overlapping independent meshes. Cut the main weather faces to the
    // tower footprint, flash every resulting edge, and bind the tower roof as
    // a child carried by its own masonry ring.
    if !square_towers.is_empty() && !assemblies.is_empty() {
        for (tower_index, tower) in square_towers.iter().copied().enumerate() {
            let Some(child_index) = assemblies
                .iter()
                .position(|roof| roof.id == RoofAssemblyId(3_000 + tower_index as u64))
            else {
                continue;
            };
            let child = assemblies[child_index].clone();
            let parent_id = assemblies[0].id;
            let cut_id = ResolvedItemId((0xF_u64 << 60) | child.id.0);
            // `SquareTower::size` locates the four authoritative wall
            // centrelines.  The parent roof must stop at the exterior shell
            // faces, not halfway through the masonry.
            let shell_half_thickness = walls
                .iter()
                .filter_map(|wall| match wall.source {
                    crate::WallSourceId::SquareTowerFace {
                        tower_index: source_tower,
                        ..
                    } if source_tower == tower_index => Some(wall.thickness_metres * 0.5),
                    _ => None,
                })
                .fold(0.0_f32, f32::max);
            let half = tower.size * 0.5 + Vec2::splat(shell_half_thickness);
            let bounds = ResolvedBounds {
                min: Vec3::new(
                    tower.centre.x - half.x,
                    assemblies[0]
                        .faces
                        .iter()
                        .flat_map(|face| face.polygon.iter().map(|point| point.y))
                        .fold(f32::INFINITY, f32::min)
                        - 0.2,
                    tower.centre.y - half.y,
                ),
                max: Vec3::new(
                    tower.centre.x + half.x,
                    tower.wall_height_metres + 8.0,
                    tower.centre.y + half.y,
                ),
            };
            geometry.voids.push(ResolvedVoid {
                id: cut_id,
                owner: assemblies[0].owner,
                bounds,
                role: VoidRole::RoofOpening,
                shape: crate::ResolvedVoidShape::Box,
                subtracts_from: assemblies[0].owner,
            });
            let cut_height = bounds.max.y;
            let mut vertical_cut = child.clone();
            let mut cut_face = child.faces[0].clone();
            cut_face.polygon = vec![
                Vec3::new(bounds.min.x, cut_height, bounds.min.z),
                Vec3::new(bounds.max.x, cut_height, bounds.min.z),
                Vec3::new(bounds.max.x, cut_height, bounds.max.z),
                Vec3::new(bounds.min.x, cut_height, bounds.max.z),
            ];
            cut_face.cutouts.clear();
            cut_face.plane = RoofPlaneEquation {
                normal: Vec3::Y,
                constant: -cut_height,
            };
            vertical_cut.faces = vec![cut_face];
            trim_roof_edge_treatments_for_cut(assemblies[0].owner, bounds, geometry);
            trim_roof_boundary_edges_for_cut(&mut assemblies[0], bounds);
            let cut_edges =
                cut_parent_roof_face(&mut assemblies[0], &vertical_cut, bounds, geometry);
            for edge in assemblies[0]
                .edges
                .iter_mut()
                .filter(|edge| cut_edges.contains(&edge.id))
            {
                edge.kind = RoofEdgeKind::TowerAbutment;
            }
            let flashing_ids = assemblies[0]
                .edges
                .iter()
                .filter(|edge| cut_edges.contains(&edge.id))
                .filter_map(|edge| edge.flashing)
                .collect::<Vec<_>>();
            assemblies[0].children.push(RoofChildAssembly {
                child: child.id,
                kind: RoofChildKind::Tower,
                parent_cut: cut_id,
                trimmer_nodes: child.support_nodes.clone(),
                valley_edges: cut_edges,
                flashing_ids,
                facade_wall: None,
                split_eave_edges: Vec::new(),
            });
            assemblies[child_index].parent = Some(parent_id);
            assemblies[child_index].phase = RoofPhase::AttachedChild;
        }
    }
    bind_coincident_primary_roof_edges(&mut assemblies, geometry);
    finalize_roof_drainage(program.archetype, &mut assemblies, geometry);
    supplement_split_eave_drainage(&assemblies, geometry);
    consolidate_roof_outlet_stations(
        program.archetype,
        &mut assemblies,
        stairs,
        walls,
        openings,
        geometry,
    );
    resolve_roof_abutment_contours(&mut assemblies, walls, geometry);
    // Tower/child clipping can shorten a verge after its treatment was first
    // resolved.  Refit the authoritative treatment to the final typed edge;
    // retaining the pre-cut bar would create a detached rod across the cut.
    let mut orphan_treatments = HashSet::new();
    for assembly in &mut assemblies {
        for treatment in geometry.solids.iter_mut().filter(|solid| {
            solid.owner == assembly.owner && solid.role == SolidRole::RoofEdgeTreatment
        }) {
            let pitch_cosine = treatment.longfall_radians.cos();
            let axis = Vec3::new(
                treatment.yaw_radians.cos() * pitch_cosine,
                treatment.longfall_radians.sin(),
                treatment.yaw_radians.sin() * pitch_cosine,
            );
            let endpoints = [
                treatment.centre - axis * treatment.size.x * 0.5,
                treatment.centre + axis * treatment.size.x * 0.5,
            ];
            let aligned = assembly.edges.iter().any(|edge| {
                if !matches!(
                    edge.kind,
                    RoofEdgeKind::Ridge | RoofEdgeKind::Hip | RoofEdgeKind::GableVerge
                ) {
                    return false;
                }
                let delta = edge.end - edge.start;
                let length_squared = delta.length_squared().max(0.000_001);
                treatment.size.x <= delta.length() + 0.03
                    && endpoints.iter().all(|point| {
                        let raw_t = (*point - edge.start).dot(delta) / length_squared;
                        let t = raw_t.clamp(0.0, 1.0);
                        point.distance(edge.start + delta * t) <= 0.075
                            && (-0.02..=1.02).contains(&raw_t)
                    })
            });
            if !aligned {
                orphan_treatments.insert(treatment.id);
            }
        }
        for edge in &mut assembly.edges {
            if edge
                .flashing
                .is_some_and(|id| orphan_treatments.contains(&id))
            {
                edge.flashing = None;
            }
        }
    }
    geometry
        .solids
        .retain(|solid| !orphan_treatments.contains(&solid.id));
    geometry.support_interfaces.retain(|interface| {
        !orphan_treatments.iter().any(|id| {
            interface.id == ResolvedItemId((0x9_u64 << 60) | (id.0 & 0x0FFF_FFFF_FFFF_FFFF))
        })
    });
    for treatment in geometry
        .solids
        .iter()
        .filter(|solid| solid.role == SolidRole::RoofEdgeTreatment)
    {
        let interface_id =
            ResolvedItemId((0x9_u64 << 60) | (treatment.id.0 & 0x0FFF_FFFF_FFFF_FFFF));
        if let Some(interface) = geometry
            .support_interfaces
            .iter_mut()
            .find(|interface| interface.id == interface_id)
        {
            interface.bounds = ResolvedBounds {
                min: treatment.centre - Vec3::new(0.08, 0.025, 0.08),
                max: treatment.centre + Vec3::new(0.08, 0.025, 0.08),
            };
        }
    }
    let roof_owners = assemblies
        .iter()
        .map(|roof| roof.owner)
        .collect::<HashSet<_>>();
    let mut roof_bonds = Vec::new();
    for left in 0..geometry.solids.len() {
        for right in left + 1..geometry.solids.len() {
            let a = &geometry.solids[left];
            let b = &geometry.solids[right];
            if a.owner == b.owner
                || (!roof_owners.contains(&a.owner) && !roof_owners.contains(&b.owner))
            {
                continue;
            }
            let yaw_bounds = |solid: &ResolvedSolid| {
                let cosine = solid.yaw_radians.cos().abs();
                let sine = solid.yaw_radians.sin().abs();
                let half = Vec3::new(
                    (solid.size.x * cosine + solid.size.z * sine) * 0.5,
                    solid.size.y * 0.5,
                    (solid.size.x * sine + solid.size.z * cosine) * 0.5,
                );
                ResolvedBounds {
                    min: solid.centre - half,
                    max: solid.centre + half,
                }
            };
            let a_bounds = yaw_bounds(a);
            let b_bounds = yaw_bounds(b);
            let min = a_bounds.min.max(b_bounds.min);
            let max = a_bounds.max.min(b_bounds.max);
            let overlap = max - min;
            if overlap.min_element() > 0.001 {
                roof_bonds.push(JunctionBond {
                    id: ResolvedItemId((0x6_u64 << 60) | roof_bonds.len() as u64),
                    owners: [a.owner, b.owner],
                    bounds: ResolvedBounds {
                        min: min - Vec3::splat(0.01),
                        max: max + Vec3::splat(0.01),
                    },
                    minimum_interface_area_square_metres: 0.005,
                    maximum_penetration_metres: overlap.x.min(overlap.z).min(0.18),
                });
            }
        }
    }
    geometry.junction_bonds.extend(roof_bonds);
    assemblies
}

fn derive_roofs(program: &BuildingProgram) -> Vec<RoofPiece> {
    let (width, depth) = program.footprint.dimensions();
    let size = Vec2::new(
        f32::from(width) * CELL_SIZE_METRES,
        f32::from(depth) * CELL_SIZE_METRES,
    );
    let top = program.storeys.len() as f32 * program.storey_height_metres;
    match (program.archetype, program.footprint) {
        (BuildingArchetype::TownHouse, _) => vec![RoofPiece {
            kind: RoofKind::Gable,
            centre: size * 0.5,
            size,
            base_height_metres: top,
            pitch_degrees: program.roof_pitch_degrees,
            ridge_axis: RidgeAxis::Z,
            eave_metres: 0.45,
            gable_profile: GableProfile::Plain,
        }],
        (BuildingArchetype::HallHouse, _) => vec![RoofPiece {
            kind: RoofKind::HalfHip,
            centre: size * 0.5,
            size,
            base_height_metres: top,
            pitch_degrees: program.roof_pitch_degrees,
            ridge_axis: RidgeAxis::Z,
            eave_metres: 0.65,
            gable_profile: GableProfile::Plain,
        }],
        (BuildingArchetype::FachwerkCottage, _) => vec![RoofPiece {
            kind: RoofKind::Gable,
            centre: size * 0.5,
            size,
            base_height_metres: top,
            pitch_degrees: program.roof_pitch_degrees,
            ridge_axis: RidgeAxis::Z,
            eave_metres: 0.5,
            gable_profile: GableProfile::Plain,
        }],
        (BuildingArchetype::FachwerkMerchantHouse, _) => vec![RoofPiece {
            kind: RoofKind::Gable,
            centre: size * 0.5,
            size,
            base_height_metres: top,
            pitch_degrees: program.roof_pitch_degrees,
            ridge_axis: RidgeAxis::Z,
            eave_metres: 0.55,
            gable_profile: GableProfile::Plain,
        }],
        (BuildingArchetype::RenaissanceTownHall, _) => vec![RoofPiece {
            kind: RoofKind::HalfHip,
            centre: size * 0.5,
            size,
            base_height_metres: top,
            pitch_degrees: program.roof_pitch_degrees,
            ridge_axis: RidgeAxis::X,
            eave_metres: 0.65,
            gable_profile: GableProfile::Stepped,
        }],
        (BuildingArchetype::Cathedral, _) => vec![
            RoofPiece {
                kind: RoofKind::Gable,
                centre: Vec2::new(21.15, 10.5),
                size: Vec2::new(31.50, 6.0),
                base_height_metres: 11.5,
                pitch_degrees: program.roof_pitch_degrees,
                ridge_axis: RidgeAxis::X,
                eave_metres: 0.55,
                gable_profile: GableProfile::Plain,
            },
            RoofPiece {
                kind: RoofKind::Shed,
                // The high edge seats on the south clerestory exterior face
                // at z=7.125 rather than passing through to its interior side.
                centre: Vec2::new(14.05, 5.5875),
                size: Vec2::new(16.40, 2.175),
                base_height_metres: 7.0,
                pitch_degrees: 28.0,
                ridge_axis: RidgeAxis::X,
                eave_metres: 0.45,
                gable_profile: GableProfile::Plain,
            },
            RoofPiece {
                kind: RoofKind::Shed,
                // Mirrored north aisle: high edge seats at z=13.875.
                centre: Vec2::new(14.05, 15.4125),
                size: Vec2::new(16.40, 2.175),
                base_height_metres: 7.0,
                pitch_degrees: 28.0,
                ridge_axis: RidgeAxis::X,
                eave_metres: 0.45,
                gable_profile: GableProfile::Plain,
            },
            RoofPiece {
                kind: RoofKind::Gable,
                centre: Vec2::new(25.65, 10.5),
                size: Vec2::new(4.5, 18.0),
                base_height_metres: 11.5,
                pitch_degrees: program.roof_pitch_degrees,
                ridge_axis: RidgeAxis::Z,
                eave_metres: 0.48,
                gable_profile: GableProfile::Plain,
            },
            RoofPiece {
                kind: RoofKind::Pavilion,
                centre: Vec2::new(39.15, 10.5),
                size: Vec2::new(8.8, 8.8),
                base_height_metres: 11.5,
                pitch_degrees: 52.0,
                ridge_axis: RidgeAxis::X,
                eave_metres: 0.40,
                gable_profile: GableProfile::Plain,
            },
        ],
        (BuildingArchetype::CastleGatehouse, _) => vec![RoofPiece {
            kind: RoofKind::Gable,
            centre: size * 0.5 + Vec2::Y * 0.5,
            // The accepted gatehouse roof is the buildable central volume
            // between the two 3 m-radius flanking towers. It may abut their
            // shells, but it must not continue behind/through either tower.
            size: Vec2::new(size.x - 6.8, size.y - 1.0),
            base_height_metres: top - 0.45,
            pitch_degrees: program.roof_pitch_degrees,
            ridge_axis: RidgeAxis::X,
            eave_metres: 0.35,
            gable_profile: GableProfile::Plain,
        }],
        (BuildingArchetype::CourtyardCastle, Footprint::Courtyard { wing, .. }) => {
            let wing_metres = f32::from(wing) * CELL_SIZE_METRES;
            // Keep the roof eaves behind the fighting circuit. The wall walk occupies
            // the outer 1.25 m of each wing and needs additional shoulder clearance.
            // The 3 m-radius corner towers own their full junction envelope.
            // Keep wing roofs beyond that envelope as well as behind the
            // fighting walk; this avoids unbuildable roof/gutter stubs through
            // the cylindrical shells.
            let outer_clearance = 3.2;
            let inner_clearance = 0.4;
            let transverse_span = wing_metres - outer_clearance - inner_clearance;
            vec![
                RoofPiece {
                    kind: RoofKind::Gable,
                    centre: Vec2::new(size.x * 0.5, outer_clearance + transverse_span * 0.5),
                    size: Vec2::new(size.x - outer_clearance * 2.0, transverse_span),
                    base_height_metres: top - 0.45,
                    pitch_degrees: program.roof_pitch_degrees,
                    ridge_axis: RidgeAxis::X,
                    eave_metres: 0.4,
                    gable_profile: GableProfile::Stepped,
                },
                RoofPiece {
                    kind: RoofKind::Gable,
                    centre: Vec2::new(
                        size.x * 0.5,
                        size.y - outer_clearance - transverse_span * 0.5,
                    ),
                    size: Vec2::new(size.x - outer_clearance * 2.0, transverse_span),
                    base_height_metres: top - 0.45,
                    pitch_degrees: program.roof_pitch_degrees,
                    ridge_axis: RidgeAxis::X,
                    eave_metres: 0.4,
                    gable_profile: GableProfile::Curved,
                },
                RoofPiece {
                    kind: RoofKind::Hip,
                    centre: Vec2::new(outer_clearance + transverse_span * 0.5, size.y * 0.5),
                    size: Vec2::new(
                        transverse_span,
                        size.y - 2.0 * (outer_clearance + transverse_span),
                    ),
                    base_height_metres: top - 0.45,
                    pitch_degrees: program.roof_pitch_degrees,
                    ridge_axis: RidgeAxis::Z,
                    eave_metres: 0.4,
                    gable_profile: GableProfile::Plain,
                },
                RoofPiece {
                    kind: RoofKind::Hip,
                    centre: Vec2::new(
                        size.x - outer_clearance - transverse_span * 0.5,
                        size.y * 0.5,
                    ),
                    size: Vec2::new(
                        transverse_span,
                        size.y - 2.0 * (outer_clearance + transverse_span),
                    ),
                    base_height_metres: top - 0.45,
                    pitch_degrees: program.roof_pitch_degrees,
                    ridge_axis: RidgeAxis::Z,
                    eave_metres: 0.4,
                    gable_profile: GableProfile::Plain,
                },
            ]
        }
        (BuildingArchetype::WalledKeep, _) => Vec::new(),
        _ => Vec::new(),
    }
}

fn derive_roof_dormers(program: &BuildingProgram) -> Vec<RoofDormer> {
    if program.roof_demonstrator == Some(RoofKind::Gable) {
        return Vec::new();
    }
    let (width, depth) = program.footprint.dimensions();
    let width = f32::from(width) * CELL_SIZE_METRES;
    let depth = f32::from(depth) * CELL_SIZE_METRES;
    let top = program.storeys.len() as f32 * program.storey_height_metres;
    let front_roof_inset = match program.footprint {
        Footprint::Courtyard { wing, .. } => f32::from(wing) * CELL_SIZE_METRES * 0.72,
        Footprint::Rectangle { .. } => 0.0,
    };
    let dormer = |centre, facing, kind, profile| RoofDormer {
        centre,
        base_height_metres: top + 1.15,
        width_metres: 2.15,
        depth_metres: 1.85,
        height_metres: 1.75,
        facing,
        kind,
        gable_profile: profile,
    };
    match program.archetype {
        BuildingArchetype::TownHouse => vec![dormer(
            Vec2::new(width, depth * 0.58),
            Direction::East,
            DormerKind::Gabled,
            GableProfile::Plain,
        )],
        BuildingArchetype::HallHouse => vec![
            dormer(
                Vec2::new(width, depth * 0.36),
                Direction::East,
                DormerKind::Shed,
                GableProfile::Plain,
            ),
            dormer(
                Vec2::new(width, depth * 0.64),
                Direction::East,
                DormerKind::Shed,
                GableProfile::Plain,
            ),
        ],
        BuildingArchetype::FachwerkCottage => vec![
            dormer(
                Vec2::new(width, depth * 0.38),
                Direction::East,
                DormerKind::Gabled,
                GableProfile::Plain,
            ),
            dormer(
                Vec2::new(width, depth * 0.66),
                Direction::East,
                DormerKind::Shed,
                GableProfile::Plain,
            ),
        ],
        BuildingArchetype::FachwerkMerchantHouse => vec![
            dormer(
                Vec2::new(width, depth * 0.38),
                Direction::East,
                DormerKind::Gabled,
                GableProfile::Plain,
            ),
            dormer(
                Vec2::new(width, depth * 0.68),
                Direction::East,
                DormerKind::Hipped,
                GableProfile::Plain,
            ),
            dormer(
                Vec2::new(0.0, depth * 0.52),
                Direction::West,
                DormerKind::TransverseGable,
                GableProfile::Plain,
            ),
        ],
        BuildingArchetype::RenaissanceTownHall => vec![
            dormer(
                Vec2::new(width * 0.22, 0.0),
                Direction::South,
                DormerKind::Gabled,
                GableProfile::Curved,
            ),
            dormer(
                Vec2::new(width * 0.78, 0.0),
                Direction::South,
                DormerKind::Gabled,
                GableProfile::Curved,
            ),
        ],
        BuildingArchetype::Cathedral => Vec::new(),
        BuildingArchetype::CastleGatehouse => Vec::new(),
        BuildingArchetype::CourtyardCastle => vec![
            dormer(
                Vec2::new(width * 0.3, front_roof_inset),
                Direction::South,
                DormerKind::Gabled,
                GableProfile::Stepped,
            ),
            dormer(
                Vec2::new(width * 0.7, front_roof_inset),
                Direction::South,
                DormerKind::Gabled,
                GableProfile::Curved,
            ),
        ],
        BuildingArchetype::WalledKeep | BuildingArchetype::ArtilleryRondelCastle => Vec::new(),
    }
}

fn derive_towers(
    program: &BuildingProgram,
    gatehouses: &[GatehouseAssemblySpec],
    curtain_walls: &[CurtainWallRun],
) -> Vec<RoundTower> {
    let (width, depth) = program.footprint.dimensions();
    let width = f32::from(width) * CELL_SIZE_METRES;
    let depth = f32::from(depth) * CELL_SIZE_METRES;
    let wall_height = program.storeys.len() as f32 * program.storey_height_metres;
    let tower = |centre: Vec2, battlement, roofed: bool| {
        RoundTower::new(
            grid_point(centre),
            CellDiameter::new(4).expect("four-cell tower diameter is valid"),
            wall_height,
            1.2,
            roofed.then_some(RoofPiece {
                kind: RoofKind::Conical,
                centre,
                size: Vec2::splat(4.7),
                base_height_metres: wall_height + 1.75,
                pitch_degrees: 58.0,
                ridge_axis: RidgeAxis::X,
                eave_metres: 0.15,
                gable_profile: GableProfile::Plain,
            }),
            battlement,
        )
        .expect("curated tower anchor matches its integral-cell footprint")
    };
    if program.roof_demonstrator == Some(RoofKind::Conical) {
        // A deterministic isolated kernel proof: a grounded round stair tower
        // alongside the civilian fixture, without changing any curated
        // archetype when the demonstrator is unset.
        return vec![tower(Vec2::new(width + 3.0, depth * 0.5), None, true)];
    }
    match program.archetype {
        BuildingArchetype::CastleGatehouse => vec![
            tower(
                Vec2::new(0.0, 0.0),
                Some(BattlementKind::Machicolated),
                false,
            )
            .with_chord_interface(TowerChordInterface {
                toward_gate: Direction::East,
                bearing_depth: GridLength::new(24).expect("1.2 m tower chord"),
            })
            .with_secondary_chord_interface(TowerChordInterface {
                toward_gate: Direction::North,
                bearing_depth: GridLength::new(24).expect("1.2 m tower return chord"),
            }),
            tower(
                Vec2::new(width, 0.0),
                Some(BattlementKind::Machicolated),
                false,
            )
            .with_chord_interface(TowerChordInterface {
                toward_gate: Direction::West,
                bearing_depth: GridLength::new(24).expect("1.2 m tower chord"),
            })
            .with_secondary_chord_interface(TowerChordInterface {
                toward_gate: Direction::North,
                bearing_depth: GridLength::new(24).expect("1.2 m tower return chord"),
            }),
        ],
        BuildingArchetype::CourtyardCastle => vec![
            tower(
                Vec2::new(0.0, 0.0),
                Some(BattlementKind::Crenellated),
                false,
            ),
            tower(
                Vec2::new(width, 0.0),
                Some(BattlementKind::Crenellated),
                false,
            ),
            tower(
                Vec2::new(0.0, depth),
                Some(BattlementKind::Crenellated),
                false,
            ),
            tower(
                Vec2::new(width, depth),
                Some(BattlementKind::Crenellated),
                false,
            ),
        ],
        BuildingArchetype::WalledKeep => {
            let margin = 9.0;
            let min = Vec2::splat(-margin);
            let max = Vec2::new(width + margin, depth + margin);
            let mut towers = [min, Vec2::new(max.x, min.y), Vec2::new(min.x, max.y), max]
                .into_iter()
                .map(|centre| {
                    RoundTower::new(
                        grid_point(centre),
                        CellDiameter::new(4).expect("four-cell tower diameter is valid"),
                        6.0,
                        1.2,
                        None,
                        Some(BattlementKind::Crenellated),
                    )
                    .expect("curtain corner tower uses a room-grid vertex")
                })
                .collect::<Vec<_>>();
            for gatehouse in gatehouses {
                if let Some(wall) = curtain_walls.get(gatehouse.curtain_wall_index)
                    && let Some(resolved) = resolve_gatehouse_towers(*gatehouse, *wall, 6.0)
                {
                    towers.extend(resolved);
                }
            }
            towers
        }
        BuildingArchetype::ArtilleryRondelCastle => {
            let diameter = CellDiameter::new(8).expect("eight-cell artillery rondel diameter");
            let bearing = GridLength::new(18).expect("0.9 m curtain return bearing");
            let make = |centre: Vec2, first: Direction, second: Direction| {
                RoundTower::new(grid_point(centre), diameter, 7.30, 1.2, None, None)
                    .expect("artillery rondel anchor matches even-cell parity")
                    .with_chord_interface(TowerChordInterface {
                        toward_gate: first,
                        bearing_depth: bearing,
                    })
                    .with_secondary_chord_interface(TowerChordInterface {
                        toward_gate: second,
                        bearing_depth: bearing,
                    })
            };
            vec![
                make(Vec2::new(-16.5, -13.5), Direction::East, Direction::North),
                make(Vec2::new(28.5, -13.5), Direction::West, Direction::North),
                make(Vec2::new(-16.5, 25.5), Direction::East, Direction::South),
                make(Vec2::new(28.5, 25.5), Direction::West, Direction::South),
            ]
        }
        _ => Vec::new(),
    }
}

fn derive_gatehouse_assemblies(program: &BuildingProgram) -> Vec<GatehouseAssemblySpec> {
    if program.archetype != BuildingArchetype::WalledKeep {
        return Vec::new();
    }
    vec![GatehouseAssemblySpec {
        curtain_wall_index: 0,
        gate_width: GridLength::new(64).expect("3.2 m project gate width"),
        tower_diameter: CellDiameter::new(4).expect("four-cell gate tower diameter"),
        tower_shell: GridLength::new(24).expect("1.2 m project shell"),
        jamb_reveal: GridLength::new(13).expect("0.65 m parity-aligned jamb reveal"),
        chord_bearing: GridLength::new(6).expect("0.3 m bonded bearing"),
        chamber_depth: GridLength::new(52).expect("2.6 m chamber depth"),
        arch_ring_depth: GridLength::new(5).expect("0.25 m masonry arch ring"),
        arch_rise: GridLength::new(8).expect("0.4 m segmental arch rise"),
        curtain_return_bond: GridLength::new(2).expect("0.1 m bonded curtain return"),
    }]
}

fn resolve_gatehouse_towers(
    spec: GatehouseAssemblySpec,
    wall: CurtainWallRun,
    wall_height: f32,
) -> Option<[RoundTower; 2]> {
    let tangent = (wall.end - wall.start).normalize_or_zero();
    let outward = direction_vector(wall.outward);
    let cardinal = tangent.x.abs() >= 0.999 || tangent.y.abs() >= 0.999;
    if !cardinal || tangent.dot(outward).abs() > 0.001 {
        return None;
    }
    let threshold = (wall.start + wall.end) * 0.5;
    let radius = spec.tower_diameter.metres() * 0.5;
    let offset = spec.gate_width.metres() * 0.5 + spec.jamb_reveal.metres() + radius;
    let left_centre = threshold - tangent * offset;
    let right_centre = threshold + tangent * offset;
    let along = cardinal_direction(tangent);
    let against = along.opposite();
    let make = |centre: Vec2, toward_gate| {
        RoundTower::new(
            grid_point(centre),
            spec.tower_diameter,
            wall_height,
            spec.tower_shell.metres(),
            None,
            Some(BattlementKind::Crenellated),
        )
        .expect("gatehouse spec must resolve parity-aligned tower anchors")
        .with_chord_interface(TowerChordInterface {
            toward_gate,
            bearing_depth: spec.chord_bearing,
        })
    };
    Some([make(left_centre, along), make(right_centre, against)])
}

fn derive_square_towers(program: &BuildingProgram) -> Vec<SquareTower> {
    if program.archetype != BuildingArchetype::Cathedral {
        return Vec::new();
    }
    let size = Vec2::splat(5.4);
    // The bell stage begins above the nave weather contour.  21.5 metres
    // keeps its paired sound openings clear of the main-roof upstand while
    // retaining a substantial masonry tower between nave ridge and bell floor.
    let wall_height_metres = 21.5;
    vec![SquareTower {
        centre: Vec2::new(2.7, 10.5),
        size,
        wall_height_metres,
        roof: RoofPiece {
            kind: RoofKind::Pavilion,
            centre: Vec2::new(2.7, 10.5),
            size,
            base_height_metres: wall_height_metres,
            pitch_degrees: 68.0,
            ridge_axis: RidgeAxis::X,
            eave_metres: 0.3,
            gable_profile: GableProfile::Plain,
        },
        bell_openings: true,
    }]
}

fn derive_stairs(
    program: &BuildingProgram,
    storeys: &[StoreyPlan],
    towers: &[RoundTower],
) -> Vec<Stair> {
    if storeys.len() < 2 {
        return Vec::new();
    }
    // A roof-kernel demonstrator may add an isolated round tower to an
    // otherwise civilian fixture. It is evidence geometry, not the occupied
    // building's circulation authority, so it must not replace the real
    // StairHall route.
    let towers_own_circulation = matches!(
        program.archetype,
        BuildingArchetype::CastleGatehouse
            | BuildingArchetype::CourtyardCastle
            | BuildingArchetype::WalledKeep
            | BuildingArchetype::ArtilleryRondelCastle
    );
    if towers_own_circulation && !towers.is_empty() {
        let mut stairs = towers
            .iter()
            .map(|tower| {
                let base_height_metres = 0.15;
                Stair::Spiral {
                    centre: tower.centre_metres(),
                    base_height_metres,
                    rise_metres: tower.wall_height_metres - base_height_metres,
                    inner_radius_metres: 0.28,
                    outer_radius_metres: (tower.radius_metres()
                        - tower.wall_thickness_metres
                        - 0.15)
                        .max(0.75),
                    turns: tower.wall_height_metres / program.storey_height_metres * 0.9,
                    clockwise: stable_noise(layout_seed(program), 11, Cell::new(0, 0))
                        .is_multiple_of(2),
                    tread_count: (tower.wall_height_metres / 0.19).ceil() as u16,
                }
            })
            .collect::<Vec<_>>();
        if matches!(
            program.archetype,
            BuildingArchetype::WalledKeep | BuildingArchetype::ArtilleryRondelCastle
        ) {
            let (width, depth) = program.footprint.dimensions();
            let base_height_metres = 0.15;
            let rise_metres =
                storeys.len() as f32 * program.storey_height_metres - base_height_metres;
            stairs.push(Stair::Spiral {
                centre: Vec2::new(
                    f32::from(width) * CELL_SIZE_METRES * 0.5,
                    f32::from(depth) * CELL_SIZE_METRES * 0.5,
                ),
                base_height_metres,
                rise_metres,
                inner_radius_metres: 0.25,
                outer_radius_metres: 1.25,
                turns: 2.8,
                clockwise: true,
                tread_count: (rise_metres / 0.19).ceil() as u16,
            });
        }
        return stairs;
    }

    let stair_room = storeys[0]
        .rooms
        .iter()
        .find(|room| room.kind == RoomKind::StairHall)
        .or_else(|| storeys[0].rooms.first());
    stair_room
        .and_then(|room| room.cells.get(room.cells.len() / 2))
        .map(|cell| {
            vec![Stair::Straight {
                start: cell.centre(),
                direction: Direction::North,
                base_height_metres: 0.12,
                rise_metres: program.storey_height_metres,
                width_metres: 1.0,
                tread_count: 17,
            }]
        })
        .unwrap_or_default()
}

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

fn crown_merlon_ranges(length: f32, profile: CrownProfile) -> Vec<(f32, f32)> {
    let minimum_end = 0.25;
    let nominal = profile.merlon_width_metres + profile.crenel_width_metres;
    let crenel_count = (((length - minimum_end * 2.0) / nominal).floor() as usize).max(1);
    let actual_merlon =
        (length - profile.crenel_width_metres * crenel_count as f32) / (crenel_count + 1) as f32;
    let mut cursor = 0.0;
    let mut ranges = Vec::with_capacity(crenel_count + 1);
    for index in 0..=crenel_count {
        ranges.push((cursor, cursor + actual_merlon));
        cursor += actual_merlon;
        if index < crenel_count {
            cursor += profile.crenel_width_metres;
        }
    }
    ranges
}

fn resolve_crown_geometry(
    crowns: &[CrownAssembly],
    walks: &[WallWalk],
    stairs: &[Stair],
    tower_portals: &[TowerPortal],
) -> ResolvedGeometry {
    let mut geometry = ResolvedGeometry {
        schema_version: 2,
        ..ResolvedGeometry::default()
    };
    for crown in crowns {
        let support_node = StructuralNodeId(u64::from(crown.owner.0) * 10 + 1);
        let p = crown.profile;
        match crown.path {
            CrownPath::Straight {
                start,
                end,
                outward,
            } => {
                let original_start = start;
                let original_end = end;
                let original_tangent = (end - start).normalize_or_zero();
                let splice_trim = |position: Vec2| {
                    crown
                        .junctions
                        .iter()
                        .find(|junction| {
                            junction.kind == CrownJunctionKind::TowerSplice
                                && (junction.position - position).length() < 0.02
                        })
                        .and_then(|junction| {
                            crowns.iter().find_map(|other| {
                                (other.owner == junction.other_owner)
                                    .then_some(other.path)
                                    .and_then(|path| match path {
                                        CrownPath::Round { radius_metres, .. } => {
                                            Some(radius_metres + p.thickness_metres * 0.5 - 0.08)
                                        }
                                        CrownPath::Straight { .. } => None,
                                    })
                            })
                        })
                        .unwrap_or(0.0)
                };
                let start = start + original_tangent * splice_trim(start);
                let end = end - original_tangent * splice_trim(end);
                let delta = end - start;
                let length = delta.length();
                let tangent = delta.normalize_or_zero();
                let normal = direction_vector(outward);
                let horizontal = tangent.x.abs() >= tangent.y.abs();
                let mut exclusions = crown
                    .junctions
                    .iter()
                    .filter_map(|junction| {
                        let other = crowns
                            .iter()
                            .find(|other| other.owner == junction.other_owner)?;
                        let CrownPath::Round { radius_metres, .. } = other.path else {
                            return None;
                        };
                        let distance = (junction.position - start).dot(tangent);
                        (distance > 0.02 && distance < length - 0.02).then_some((
                            (distance - radius_metres - p.thickness_metres * 0.5 + 0.08).max(0.0),
                            (distance + radius_metres + p.thickness_metres * 0.5 - 0.08)
                                .min(length),
                        ))
                    })
                    .collect::<Vec<_>>();
                exclusions.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut active_ranges = Vec::new();
                let mut active_start = 0.0;
                for (cut_start, cut_end) in exclusions {
                    if cut_start > active_start + 0.02 {
                        active_ranges.push((active_start, cut_start));
                    }
                    active_start = active_start.max(cut_end);
                }
                if length > active_start + 0.02 {
                    active_ranges.push((active_start, length));
                }
                let solid = |role, along_centre: f32, along_size: f32, z: f32, height: f32| {
                    let plan = start + tangent * along_centre + normal * p.thickness_metres * 0.5;
                    let transverse =
                        p.thickness_metres + if role == SolidRole::Coping { 0.04 } else { 0.0 };
                    ResolvedSolid {
                        id: ResolvedItemId::default(),
                        owner: crown.owner,
                        centre: Vec3::new(plan.x, z + height * 0.5, plan.y),
                        size: if horizontal {
                            Vec3::new(along_size, height, transverse)
                        } else {
                            Vec3::new(transverse, height, along_size)
                        },
                        yaw_radians: 0.0,
                        crossfall_radians: 0.0,
                        longfall_radians: 0.0,
                        role,
                        shape: crate::ResolvedSolidShape::Cuboid,
                        supported_by: vec![support_node],
                    }
                };
                let drain_height = 0.18;
                for &(range_start, range_end) in &active_ranges {
                    let mut chunk_start = range_start;
                    while chunk_start < range_end - 0.01 {
                        let chunk_end = (chunk_start + 2.4).min(range_end);
                        geometry.solids.push(solid(
                            SolidRole::Breastwork,
                            (chunk_start + chunk_end) * 0.5,
                            chunk_end - chunk_start,
                            crown.base_height_metres + drain_height,
                            p.breastwork_height_metres - drain_height,
                        ));
                        geometry.solids.push(solid(
                            SolidRole::Coping,
                            (chunk_start + chunk_end) * 0.5,
                            chunk_end - chunk_start,
                            crown.base_height_metres + p.breastwork_height_metres,
                            p.coping_height_metres,
                        ));
                        let guard_start_trim = crown.junctions.iter().any(|junction| {
                            junction.kind == CrownJunctionKind::Corner
                                && (junction.position - original_start).length() < 0.02
                        });
                        let guard_end_trim = crown.junctions.iter().any(|junction| {
                            junction.kind == CrownJunctionKind::Corner
                                && (junction.position - original_end).length() < 0.02
                        });
                        let guard_start = chunk_start.max(if guard_start_trim {
                            p.walk_clear_width_metres
                        } else {
                            0.0
                        });
                        let guard_end = chunk_end.min(if guard_end_trim {
                            length - p.walk_clear_width_metres
                        } else {
                            length
                        });
                        if guard_end > guard_start + 0.02 {
                            let inner_guard_plan = start
                                + tangent * ((guard_start + guard_end) * 0.5)
                                - normal * (p.walk_clear_width_metres + 0.08);
                            geometry.solids.push(ResolvedSolid {
                                id: ResolvedItemId::default(),
                                owner: crown.owner,
                                centre: Vec3::new(
                                    inner_guard_plan.x,
                                    crown.base_height_metres + p.inner_guard_height_metres * 0.5,
                                    inner_guard_plan.y,
                                ),
                                size: if horizontal {
                                    Vec3::new(
                                        guard_end - guard_start,
                                        p.inner_guard_height_metres,
                                        0.12,
                                    )
                                } else {
                                    Vec3::new(
                                        0.12,
                                        p.inner_guard_height_metres,
                                        guard_end - guard_start,
                                    )
                                },
                                yaw_radians: 0.0,
                                crossfall_radians: 0.0,
                                longfall_radians: 0.0,
                                role: SolidRole::EdgeGuard,
                                shape: crate::ResolvedSolidShape::Cuboid,
                                supported_by: vec![support_node],
                            });
                        }
                        chunk_start = chunk_end;
                    }
                }
                let mut cuts = crown
                    .drain_positions
                    .iter()
                    .map(|drain| (*drain - start).dot(tangent))
                    .filter(|distance| {
                        active_ranges.iter().any(|(range_start, range_end)| {
                            *distance > *range_start + 0.08 && *distance < *range_end - 0.08
                        })
                    })
                    .collect::<Vec<_>>();
                cuts.sort_by(f32::total_cmp);
                for &(range_start, range_end) in &active_ranges {
                    let mut cursor = range_start;
                    for cut in cuts
                        .iter()
                        .copied()
                        .filter(|cut| *cut > range_start && *cut < range_end)
                        .chain(std::iter::once(range_end + 0.08))
                    {
                        let end = (cut - 0.08).min(range_end);
                        if end - cursor > 0.02 {
                            geometry.solids.push(solid(
                                SolidRole::Breastwork,
                                (cursor + end) * 0.5,
                                end - cursor,
                                crown.base_height_metres,
                                drain_height,
                            ));
                        }
                        cursor = (cut + 0.08).min(range_end);
                    }
                }
                for (from, to) in crown_merlon_ranges(length, p)
                    .into_iter()
                    .filter(|(from, to)| {
                        let start_owned = crown
                            .junctions
                            .iter()
                            .any(|junction| (junction.position - original_start).length() < 0.02);
                        let end_owned = crown
                            .junctions
                            .iter()
                            .any(|junction| (junction.position - original_end).length() < 0.02);
                        !(start_owned && *from < 0.02) && !(end_owned && length - *to < 0.02)
                    })
                    .flat_map(|(from, to)| {
                        active_ranges
                            .iter()
                            .filter_map(move |(range_start, range_end)| {
                                let clipped_from = from.max(*range_start);
                                let clipped_to = to.min(*range_end);
                                (clipped_to - clipped_from >= 0.25)
                                    .then_some((clipped_from, clipped_to))
                            })
                    })
                {
                    geometry.solids.push(solid(
                        SolidRole::Merlon,
                        (from + to) * 0.5,
                        to - from,
                        crown.base_height_metres
                            + p.breastwork_height_metres
                            + p.coping_height_metres,
                        p.merlon_height_metres - p.coping_height_metres,
                    ));
                    geometry.solids.push(solid(
                        SolidRole::Coping,
                        (from + to) * 0.5,
                        to - from,
                        crown.base_height_metres
                            + p.breastwork_height_metres
                            + p.merlon_height_metres,
                        p.coping_height_metres,
                    ));
                }
                let walk = walks.iter().find(|walk| matches!(walk, WallWalk::Linear { start: a, end: b, .. } if (*a-original_start).length()<0.02 && (*b-original_end).length()<0.02));
                if let Some(walk) = walk {
                    let bounds = linear_walk_bounds_for_geometry(*walk);
                    geometry.surfaces.push(ResolvedSurface {
                        id: ResolvedItemId::default(),
                        owner: crown.owner,
                        bounds,
                        role: SurfaceRole::Stance,
                        shape: crate::ResolvedSurfaceShape::Planar,
                    });
                }
                for drain in crown.drain_positions.iter().filter(|drain| {
                    let distance = (**drain - start).dot(tangent);
                    active_ranges.iter().any(|(range_start, range_end)| {
                        distance > *range_start + 0.08 && distance < *range_end - 0.08
                    })
                }) {
                    let inner = *drain + normal * 0.01;
                    let outer = *drain + normal * (p.thickness_metres + 0.01);
                    let lateral = tangent.abs() * 0.06;
                    geometry.voids.push(ResolvedVoid {
                        id: ResolvedItemId::default(),
                        owner: crown.owner,
                        bounds: ResolvedBounds {
                            min: Vec3::new(
                                inner.x.min(outer.x) - lateral.x,
                                crown.base_height_metres,
                                inner.y.min(outer.y) - lateral.y,
                            ),
                            max: Vec3::new(
                                inner.x.max(outer.x) + lateral.x,
                                crown.base_height_metres + 0.18,
                                inner.y.max(outer.y) + lateral.y,
                            ),
                        },
                        role: VoidRole::Drain,
                        shape: crate::ResolvedVoidShape::Box,
                        subtracts_from: crown.owner,
                    });
                }
                geometry.surfaces.push(ResolvedSurface {
                    id: ResolvedItemId::default(),
                    owner: crown.owner,
                    bounds: ResolvedBounds {
                        min: Vec3::new(
                            start.x.min(end.x),
                            crown.base_height_metres + p.firing_height_metres,
                            start.y.min(end.y),
                        ),
                        max: Vec3::new(
                            start.x.max(end.x),
                            crown.base_height_metres + p.firing_height_metres + 0.01,
                            start.y.max(end.y),
                        ),
                    },
                    role: SurfaceRole::FiringLine,
                    shape: crate::ResolvedSurfaceShape::Planar,
                });
            }
            CrownPath::Round {
                tower_index,
                centre,
                radius_metres,
            } => {
                let segments = 24;
                let segment_angle = std::f32::consts::TAU / segments as f32;
                let mut portal_angles = tower_portals
                    .iter()
                    .filter(|portal| {
                        portal.tower_index == tower_index
                            && matches!(portal.kind, TowerPortalKind::WallWalkJunction { .. })
                    })
                    .map(|portal| {
                        let facing = direction_vector(portal.facing);
                        facing.y.atan2(facing.x)
                    })
                    .collect::<Vec<_>>();
                // Gate towers can splice into the middle of a straight crown
                // without owning a defensive-circuit portal. Resolve both
                // wallward directions from the reciprocal crown junction so
                // their circular breastwork and merlons are cut as well.
                for junction in crown
                    .junctions
                    .iter()
                    .filter(|junction| junction.kind == CrownJunctionKind::TowerSplice)
                {
                    if let Some(CrownPath::Straight { start, end, .. }) = crowns
                        .iter()
                        .find(|other| other.owner == junction.other_owner)
                        .map(|other| other.path)
                    {
                        for point in [start, end] {
                            let direction = point - centre;
                            if direction.length() > radius_metres + 0.1 {
                                portal_angles.push(direction.y.atan2(direction.x));
                            }
                        }
                    }
                }
                let angular_distance = |a: f32, b: f32| {
                    ((a - b + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI)
                        .abs()
                };
                let outer_sector_open = |angle: f32| {
                    portal_angles.iter().any(|portal_angle| {
                        angular_distance(angle, *portal_angle)
                            <= segment_angle + 0.45 / radius_metres
                    })
                };
                // A merlon whose centre is outside the clear portal can still
                // project across its edge. Expand the centre exclusion by the
                // half-merlon chord so the masonry cannot overlap the entering
                // straight crown or narrow the declared route.
                let outer_merlon_sector_open = |angle: f32| {
                    portal_angles.iter().any(|portal_angle| {
                        angular_distance(angle, *portal_angle)
                            <= segment_angle + (0.45 + p.merlon_width_metres * 0.5) / radius_metres
                    })
                };
                let stair_arrival = stairs.iter().find_map(|stair| match *stair {
                    Stair::Spiral {
                        centre: stair_centre,
                        turns,
                        clockwise,
                        tread_count,
                        ..
                    } if (stair_centre - centre).length() < 0.02 => {
                        let progress = f32::from(tread_count.saturating_sub(1))
                            / f32::from(tread_count.max(1));
                        let handedness = if clockwise { -1.0 } else { 1.0 };
                        Some(handedness * progress * turns * std::f32::consts::TAU)
                    }
                    _ => None,
                });
                for index in 0..segments {
                    let angle = index as f32 * std::f32::consts::TAU / segments as f32;
                    if outer_sector_open(angle) {
                        continue;
                    }
                    let radial = Vec2::new(angle.cos(), angle.sin());
                    let tangent_length = std::f32::consts::TAU * radius_metres / segments as f32;
                    let plan = centre + radial * (radius_metres + p.thickness_metres * 0.5);
                    geometry.solids.push(ResolvedSolid {
                        id: ResolvedItemId::default(),
                        owner: crown.owner,
                        centre: Vec3::new(
                            plan.x,
                            crown.base_height_metres
                                + 0.18
                                + (p.breastwork_height_metres - 0.18) * 0.5,
                            plan.y,
                        ),
                        size: Vec3::new(
                            tangent_length + 0.03,
                            p.breastwork_height_metres - 0.18,
                            p.thickness_metres,
                        ),
                        yaw_radians: -angle - std::f32::consts::FRAC_PI_2,
                        crossfall_radians: 0.0,
                        longfall_radians: 0.0,
                        role: SolidRole::Breastwork,
                        shape: crate::ResolvedSolidShape::Cuboid,
                        supported_by: vec![support_node],
                    });
                    // Every third segment is a genuine open scupper through the
                    // lower breastwork band, aligned with the eight declared drains.
                    if index % 3 != 0 {
                        geometry.solids.push(ResolvedSolid {
                            id: ResolvedItemId::default(),
                            owner: crown.owner,
                            centre: Vec3::new(plan.x, crown.base_height_metres + 0.09, plan.y),
                            size: Vec3::new(tangent_length + 0.03, 0.18, p.thickness_metres),
                            yaw_radians: -angle - std::f32::consts::FRAC_PI_2,
                            crossfall_radians: 0.0,
                            longfall_radians: 0.0,
                            role: SolidRole::Breastwork,
                            shape: crate::ResolvedSolidShape::Cuboid,
                            supported_by: vec![support_node],
                        });
                    }
                    geometry.solids.push(ResolvedSolid {
                        id: ResolvedItemId::default(),
                        owner: crown.owner,
                        centre: Vec3::new(
                            plan.x,
                            crown.base_height_metres
                                + p.breastwork_height_metres
                                + p.coping_height_metres * 0.5,
                            plan.y,
                        ),
                        size: Vec3::new(
                            tangent_length + 0.03,
                            p.coping_height_metres,
                            p.thickness_metres + 0.04,
                        ),
                        yaw_radians: -angle - std::f32::consts::FRAC_PI_2,
                        crossfall_radians: 0.0,
                        longfall_radians: 0.0,
                        role: SolidRole::Coping,
                        shape: crate::ResolvedSolidShape::Cuboid,
                        supported_by: vec![support_node],
                    });
                }
                let circumference = std::f32::consts::TAU * radius_metres;
                let merlon_count = (circumference / (p.merlon_width_metres + p.crenel_width_metres))
                    .floor()
                    .max(4.0) as usize;
                let pitch = circumference / merlon_count as f32;
                let merlon_width = pitch - p.crenel_width_metres;
                for index in 0..merlon_count {
                    let angle = index as f32 * std::f32::consts::TAU / merlon_count as f32;
                    if outer_merlon_sector_open(angle) {
                        continue;
                    }
                    let radial = Vec2::new(angle.cos(), angle.sin());
                    let plan = centre + radial * (radius_metres + p.thickness_metres * 0.5);
                    geometry.solids.push(ResolvedSolid {
                        id: ResolvedItemId::default(),
                        owner: crown.owner,
                        centre: Vec3::new(
                            plan.x,
                            crown.base_height_metres
                                + p.breastwork_height_metres
                                + p.merlon_height_metres * 0.5,
                            plan.y,
                        ),
                        size: Vec3::new(merlon_width, p.merlon_height_metres, p.thickness_metres),
                        yaw_radians: -angle - std::f32::consts::FRAC_PI_2,
                        crossfall_radians: 0.0,
                        longfall_radians: 0.0,
                        role: SolidRole::Merlon,
                        shape: crate::ResolvedSolidShape::Cuboid,
                        supported_by: vec![support_node],
                    });
                    geometry.solids.push(ResolvedSolid {
                        id: ResolvedItemId::default(),
                        owner: crown.owner,
                        centre: Vec3::new(
                            plan.x,
                            crown.base_height_metres
                                + p.breastwork_height_metres
                                + p.merlon_height_metres
                                + p.coping_height_metres * 0.5,
                            plan.y,
                        ),
                        size: Vec3::new(
                            merlon_width,
                            p.coping_height_metres,
                            p.thickness_metres + 0.04,
                        ),
                        yaw_radians: -angle - std::f32::consts::FRAC_PI_2,
                        crossfall_radians: 0.0,
                        longfall_radians: 0.0,
                        role: SolidRole::Coping,
                        shape: crate::ResolvedSolidShape::Cuboid,
                        supported_by: vec![support_node],
                    });
                }
                if let Some(WallWalk::Round {
                    stairwell_radius_metres,
                    ..
                }) = walks.iter().find(|walk| matches!(walk, WallWalk::Round { centre: walk_centre, .. } if (*walk_centre-centre).length()<0.02))
                {
                    for index in 0..24 {
                        let angle = index as f32 * std::f32::consts::TAU / 24.0;
                        let radial = Vec2::new(angle.cos(), angle.sin());
                        let radius = *stairwell_radius_metres + 0.08;
                        if stair_arrival.is_some_and(|arrival| {
                            angular_distance(angle, arrival)
                                <= segment_angle + 0.45 / radius
                        }) {
                            continue;
                        }
                        let plan = centre + radial * radius;
                        geometry.solids.push(ResolvedSolid {
                            id: ResolvedItemId::default(),
                            owner: crown.owner,
                            centre: Vec3::new(
                                plan.x,
                                crown.base_height_metres + p.inner_guard_height_metres * 0.5,
                                plan.y,
                            ),
                            size: Vec3::new(
                                std::f32::consts::TAU * radius / 24.0 + 0.02,
                                p.inner_guard_height_metres,
                                0.12,
                            ),
                            yaw_radians: -angle - std::f32::consts::FRAC_PI_2,
                            crossfall_radians: 0.0,
                            longfall_radians: 0.0,
                            role: SolidRole::EdgeGuard,
                            shape: crate::ResolvedSolidShape::Cuboid,
                            supported_by: vec![support_node],
                        });
                    }
                }
                geometry.surfaces.push(ResolvedSurface {
                    id: ResolvedItemId::default(),
                    owner: crown.owner,
                    bounds: ResolvedBounds {
                        min: Vec3::new(
                            centre.x - radius_metres,
                            crown.base_height_metres - 0.08,
                            centre.y - radius_metres,
                        ),
                        max: Vec3::new(
                            centre.x + radius_metres,
                            crown.base_height_metres,
                            centre.y + radius_metres,
                        ),
                    },
                    role: SurfaceRole::Stance,
                    shape: crate::ResolvedSurfaceShape::Planar,
                });
                geometry.surfaces.push(ResolvedSurface {
                    id: ResolvedItemId::default(),
                    owner: crown.owner,
                    bounds: ResolvedBounds {
                        min: Vec3::new(
                            centre.x - radius_metres,
                            crown.base_height_metres + p.firing_height_metres,
                            centre.y - radius_metres,
                        ),
                        max: Vec3::new(
                            centre.x + radius_metres,
                            crown.base_height_metres + p.firing_height_metres + 0.01,
                            centre.y + radius_metres,
                        ),
                    },
                    role: SurfaceRole::FiringLine,
                    shape: crate::ResolvedSurfaceShape::Planar,
                });
                for drain in crown.drain_positions.iter().filter(|drain| {
                    let radial = **drain - centre;
                    !outer_sector_open(radial.y.atan2(radial.x))
                }) {
                    let outward = (*drain - centre).normalize_or_zero();
                    let tangent = Vec2::new(-outward.y, outward.x);
                    let inner = *drain + outward * 0.01;
                    let outer = *drain + outward * (p.thickness_metres + 0.01);
                    let lateral = tangent.abs() * 0.06;
                    geometry.voids.push(ResolvedVoid {
                        id: ResolvedItemId::default(),
                        owner: crown.owner,
                        bounds: ResolvedBounds {
                            min: Vec3::new(
                                inner.x.min(outer.x) - lateral.x,
                                crown.base_height_metres,
                                inner.y.min(outer.y) - lateral.y,
                            ),
                            max: Vec3::new(
                                inner.x.max(outer.x) + lateral.x,
                                crown.base_height_metres + 0.18,
                                inner.y.max(outer.y) + lateral.y,
                            ),
                        },
                        role: VoidRole::Drain,
                        shape: crate::ResolvedVoidShape::Box,
                        subtracts_from: crown.owner,
                    });
                }
            }
        }
        for junction in crown.junctions.iter().filter(|junction| {
            junction.kind == CrownJunctionKind::Corner && crown.owner.0 < junction.other_owner.0
        }) {
            geometry.solids.push(ResolvedSolid {
                id: ResolvedItemId::default(),
                owner: crown.owner,
                centre: Vec3::new(
                    junction.position.x,
                    crown.base_height_metres
                        + p.breastwork_height_metres
                        + p.coping_height_metres
                        + (p.merlon_height_metres - p.coping_height_metres) * 0.5,
                    junction.position.y,
                ),
                size: Vec3::new(
                    p.merlon_width_metres,
                    p.merlon_height_metres - p.coping_height_metres,
                    p.merlon_width_metres,
                ),
                yaw_radians: 0.0,
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::Merlon,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: vec![support_node],
            });
            geometry.solids.push(ResolvedSolid {
                id: ResolvedItemId::default(),
                owner: crown.owner,
                centre: Vec3::new(
                    junction.position.x,
                    crown.base_height_metres
                        + p.breastwork_height_metres
                        + p.merlon_height_metres
                        + p.coping_height_metres * 0.5,
                    junction.position.y,
                ),
                size: Vec3::new(
                    p.merlon_width_metres + 0.04,
                    p.coping_height_metres,
                    p.merlon_width_metres + 0.04,
                ),
                yaw_radians: 0.0,
                crossfall_radians: 0.0,
                longfall_radians: 0.0,
                role: SolidRole::Coping,
                shape: crate::ResolvedSolidShape::Cuboid,
                supported_by: vec![support_node],
            });
        }
        geometry.structural_nodes.push(StructuralNode {
            id: support_node,
            owner: crown.owner,
            kind: if matches!(crown.path, CrownPath::Round { .. }) {
                StructuralNodeKind::TowerShellBearing
            } else {
                StructuralNodeKind::WallBearing
            },
            position: match crown.path {
                CrownPath::Straight { start, end, .. } => Vec3::new(
                    (start.x + end.x) * 0.5,
                    crown.base_height_metres,
                    (start.y + end.y) * 0.5,
                ),
                CrownPath::Round { centre, .. } => {
                    Vec3::new(centre.x, crown.base_height_metres, centre.y)
                }
            },
            supported_by: Vec::new(),
            grounded: true,
        });
    }
    for (index, solid) in geometry.solids.iter_mut().enumerate() {
        solid.id = ResolvedItemId((1_u64 << 60) | (u64::from(solid.owner.0) << 32) | index as u64);
        if solid.role == SolidRole::Coping {
            solid.crossfall_radians = 0.045;
        }
        let bounds = resolved_axis_bounds(solid.centre, solid.size);
        geometry.support_interfaces.push(SupportInterface {
            id: ResolvedItemId((4_u64 << 60) | index as u64),
            owner: solid.owner,
            node: solid.supported_by[0],
            bounds: ResolvedBounds {
                min: Vec3::new(bounds.min.x, bounds.min.y - 0.015, bounds.min.z),
                max: Vec3::new(bounds.max.x, bounds.min.y + 0.015, bounds.max.z),
            },
        });
    }
    for (index, surface) in geometry.surfaces.iter_mut().enumerate() {
        surface.id =
            ResolvedItemId((2_u64 << 60) | (u64::from(surface.owner.0) << 32) | index as u64);
    }
    for (index, void) in geometry.voids.iter_mut().enumerate() {
        void.id = ResolvedItemId((3_u64 << 60) | (u64::from(void.owner.0) << 32) | index as u64);
        let crown = crowns
            .iter()
            .find(|crown| crown.owner == void.owner)
            .expect("resolved void crown owner");
        let centre = (void.bounds.min + void.bounds.max) * 0.5;
        let outward = match crown.path {
            CrownPath::Straight { outward, .. } => direction_vector(outward),
            CrownPath::Round { centre: tower, .. } => {
                (Vec2::new(centre.x, centre.z) - tower).normalize_or_zero()
            }
        };
        geometry.drainage_routes.push(DrainageRoute {
            id: ResolvedItemId((5_u64 << 60) | index as u64),
            owner: void.owner,
            outlet_void: void.id,
            inlet: Vec3::new(
                centre.x - outward.x * (crown.profile.thickness_metres * 0.5 + 0.01),
                crown.base_height_metres - 0.02,
                centre.z - outward.y * (crown.profile.thickness_metres * 0.5 + 0.01),
            ),
            outlet: Vec3::new(
                centre.x + outward.x * 0.35,
                crown.base_height_metres - 0.08,
                centre.z + outward.y * 0.35,
            ),
        });
    }
    // The wall-walk catchment is resolved as physical geometry rather than as
    // a nominal drainage arrow. Local +X follows the walk and local +Z is the
    // transverse axis; the signed crossfall therefore has one unambiguous
    // downhill direction in every cardinal orientation. The 60 mm fall is a
    // project readability/drainage gate, not a universal historical dimension.
    for crown in crowns {
        let routes = geometry
            .drainage_routes
            .iter()
            .filter(|route| route.owner == crown.owner)
            .copied()
            .collect::<Vec<_>>();
        if routes.is_empty() {
            continue;
        }
        match crown.path {
            CrownPath::Straight {
                start,
                end,
                outward,
            } => {
                let tangent = (end - start).normalize_or_zero();
                let outward = direction_vector(outward);
                let length = (end - start).length();
                // At a right-angle corner one walk owns the shared square and
                // the other butts into it. Owner ordering is deterministic and
                // avoids two independently rendered slabs occupying the same
                // volume while preserving continuous walking area.
                let delegated_corner_trim = |position: Vec2| {
                    crown
                        .junctions
                        .iter()
                        .find(|junction| {
                            junction.kind == CrownJunctionKind::Corner
                                && (junction.position - position).length() < 0.02
                                && crown.owner > junction.other_owner
                        })
                        .map_or(0.0, |_| {
                            crown.profile.walk_clear_width_metres + crown.profile.thickness_metres
                        })
                };
                let start_trim = delegated_corner_trim(start);
                let end_trim = delegated_corner_trim(end);
                let mut exclusions = crown
                    .junctions
                    .iter()
                    .filter_map(|junction| {
                        let other = crowns
                            .iter()
                            .find(|other| other.owner == junction.other_owner)?;
                        let CrownPath::Round { radius_metres, .. } = other.path else {
                            return None;
                        };
                        let distance = (junction.position - start).dot(tangent);
                        (distance >= -0.02 && distance <= length + 0.02).then_some((
                            (distance - radius_metres - crown.profile.thickness_metres * 0.5
                                + 0.08)
                                .max(0.0),
                            (distance + radius_metres + crown.profile.thickness_metres * 0.5
                                - 0.08)
                                .min(length),
                        ))
                    })
                    .collect::<Vec<_>>();
                exclusions.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut active_ranges = Vec::new();
                let mut cursor = start_trim;
                for (cut_start, cut_end) in exclusions {
                    if cut_start > cursor + 0.02 {
                        active_ranges.push((cursor, cut_start));
                    }
                    cursor = cursor.max(cut_end);
                }
                let owned_end = length - end_trim;
                if owned_end > cursor + 0.02 {
                    active_ranges.push((cursor, owned_end));
                }
                for (range_start, range_end) in active_ranges {
                    let mut basin_routes = routes
                        .iter()
                        .map(|route| {
                            let along =
                                (Vec2::new(route.inlet.x, route.inlet.z) - start).dot(tangent);
                            (along, *route)
                        })
                        .filter(|(along, _)| {
                            *along >= range_start - 0.02 && *along <= range_end + 0.02
                        })
                        .collect::<Vec<_>>();
                    basin_routes.sort_by(|a, b| a.0.total_cmp(&b.0));
                    for (index, (along, route)) in basin_routes.iter().enumerate() {
                        let left = if index == 0 {
                            range_start
                        } else {
                            (basin_routes[index - 1].0 + *along) * 0.5
                        };
                        let right = if index + 1 == basin_routes.len() {
                            range_end
                        } else {
                            (*along + basin_routes[index + 1].0) * 0.5
                        };
                        for (half_start, half_end) in [(left, *along), (*along, right)] {
                            if half_end <= half_start + 0.03 {
                                continue;
                            }
                            let centre = start + tangent * ((half_start + half_end) * 0.5)
                                - outward
                                    * (crown.profile.walk_clear_width_metres * 0.5
                                        + crown.profile.thickness_metres * 0.5);
                            push_drainage_catchment(
                                &mut geometry,
                                crown,
                                *route,
                                centre,
                                tangent,
                                outward,
                                half_end - half_start,
                            );
                        }
                    }
                }
            }
            CrownPath::Round {
                centre,
                radius_metres,
                ..
            } => {
                let segment_count = 24;
                // Faceted deck chords sit slightly inside the mathematical
                // circle so their outer corners do not obstruct the scupper
                // mouths in the round breastwork.
                let deck_radius = radius_metres
                    - crown.profile.thickness_metres * 0.5
                    - crown.profile.walk_clear_width_metres * 0.5
                    - 0.03;
                let outer_walk_radius = deck_radius + crown.profile.walk_clear_width_metres * 0.5;
                for index in 0..segment_count {
                    let angle = index as f32 * std::f32::consts::TAU / segment_count as f32;
                    let outward = Vec2::new(angle.cos(), angle.sin());
                    let tangent = Vec2::new(-outward.y, outward.x);
                    let segment_centre = centre + outward * deck_radius;
                    let route = routes
                        .iter()
                        .min_by(|a, b| {
                            let a_direction = Vec2::new(a.outlet.x, a.outlet.z) - centre;
                            let b_direction = Vec2::new(b.outlet.x, b.outlet.z) - centre;
                            let a_dot = a_direction.normalize_or_zero().dot(outward);
                            let b_dot = b_direction.normalize_or_zero().dot(outward);
                            b_dot.total_cmp(&a_dot)
                        })
                        .expect("round crown has a drainage route");
                    let full_length = 2.0
                        * outer_walk_radius
                        * (std::f32::consts::PI / segment_count as f32).tan()
                        + 0.02;
                    for side in [-1.0_f32, 1.0] {
                        push_drainage_catchment(
                            &mut geometry,
                            crown,
                            *route,
                            segment_centre + tangent * side * full_length * 0.25,
                            tangent,
                            outward,
                            full_length * 0.5,
                        );
                    }
                }
            }
        }
    }
    for crown in crowns {
        match crown.path {
            CrownPath::Straight {
                start,
                end,
                outward,
            } => {
                let normal = direction_vector(outward);
                let mut accepted = Vec::new();
                for step in 1..120 {
                    let sample = step as f32 / 120.0;
                    let line = start.lerp(end, sample);
                    let firing_point = Vec3::new(
                        line.x + normal.x * crown.profile.thickness_metres * 0.5,
                        crown.base_height_metres + crown.profile.firing_height_metres,
                        line.y + normal.y * crown.profile.thickness_metres * 0.5,
                    );
                    let blocked = geometry.solids.iter().any(|solid| {
                        solid.owner == crown.owner && solid.role == SolidRole::Merlon && {
                            resolved_solid_contains_point(solid, firing_point, 0.005)
                        }
                    });
                    if !blocked
                        && accepted
                            .last()
                            .is_none_or(|previous: &Vec2| previous.distance(line) >= 1.0)
                    {
                        accepted.push(line);
                    }
                    if accepted.len() == 3 {
                        break;
                    }
                }
                for line in accepted {
                    let stance = line - normal * 0.55;
                    geometry.defender_samples.push(DefenderSample {
                        owner: crown.owner,
                        stance: Vec3::new(stance.x, crown.base_height_metres, stance.y),
                        eye: Vec3::new(stance.x, crown.base_height_metres + 1.62, stance.y),
                        target: Vec3::new(
                            line.x + normal.x * 12.0,
                            crown.base_height_metres + 1.2,
                            line.y + normal.y * 12.0,
                        ),
                    });
                }
            }
            CrownPath::Round {
                centre,
                radius_metres,
                ..
            } => {
                let mut accepted = 0;
                for sample in 0..48 {
                    let angle = sample as f32 * std::f32::consts::TAU / 48.0;
                    let radial = Vec2::new(angle.cos(), angle.sin());
                    let firing_point = Vec3::new(
                        centre.x
                            + radial.x * (radius_metres + crown.profile.thickness_metres * 0.5),
                        crown.base_height_metres + crown.profile.firing_height_metres,
                        centre.y
                            + radial.y * (radius_metres + crown.profile.thickness_metres * 0.5),
                    );
                    let blocked = geometry.solids.iter().any(|solid| {
                        solid.owner == crown.owner && solid.role == SolidRole::Merlon && {
                            resolved_solid_contains_point(solid, firing_point, 0.005)
                        }
                    });
                    if blocked {
                        continue;
                    }
                    let stance = centre + radial * (radius_metres - 0.55);
                    geometry.defender_samples.push(DefenderSample {
                        owner: crown.owner,
                        stance: Vec3::new(stance.x, crown.base_height_metres, stance.y),
                        eye: Vec3::new(stance.x, crown.base_height_metres + 1.62, stance.y),
                        target: Vec3::new(
                            centre.x + radial.x * (radius_metres + 12.0),
                            crown.base_height_metres + 1.2,
                            centre.y + radial.y * (radius_metres + 12.0),
                        ),
                    });
                    accepted += 1;
                    if accepted == 8 {
                        break;
                    }
                }
            }
        }
    }
    let mut seen_bonds = std::collections::BTreeSet::new();
    for crown in crowns {
        for junction in &crown.junctions {
            let pair = if crown.owner < junction.other_owner {
                [crown.owner, junction.other_owner]
            } else {
                [junction.other_owner, crown.owner]
            };
            let (mut positions, tangent) = match crown.path {
                CrownPath::Straight { start, end, .. }
                    if junction.kind == CrownJunctionKind::TowerSplice =>
                {
                    let tangent = (end - start).normalize_or_zero();
                    let Some((tower_centre, tower_radius)) = crowns
                        .iter()
                        .find_map(|other| {
                            (other.owner == junction.other_owner).then_some(other.path)
                        })
                        .and_then(|path| match path {
                            CrownPath::Round {
                                centre,
                                radius_metres,
                                ..
                            } => Some((centre, radius_metres)),
                            CrownPath::Straight { .. } => None,
                        })
                    else {
                        continue;
                    };
                    let offset = tower_radius + crown.profile.thickness_metres * 0.5 - 0.08;
                    let positions = if (junction.position - start).length() < 0.02 {
                        vec![tower_centre + tangent * offset]
                    } else if (junction.position - end).length() < 0.02 {
                        vec![tower_centre - tangent * offset]
                    } else {
                        vec![
                            tower_centre - tangent * offset,
                            tower_centre + tangent * offset,
                        ]
                    };
                    (positions, Some(tangent))
                }
                CrownPath::Round { .. } if junction.kind == CrownJunctionKind::TowerSplice => {
                    continue;
                }
                _ => (vec![junction.position], None),
            };
            if let CrownPath::Straight { outward, .. } = crown.path {
                let inward =
                    -direction_vector(outward) * (crown.profile.walk_clear_width_metres + 0.08);
                if junction.kind == CrownJunctionKind::TowerSplice {
                    let guard_positions = positions
                        .iter()
                        .map(|position| *position + inward)
                        .collect::<Vec<_>>();
                    positions.extend(guard_positions);
                } else if let Some(other_outward) = crowns
                    .iter()
                    .find_map(|other| (other.owner == junction.other_owner).then_some(other.path))
                    .and_then(|path| match path {
                        CrownPath::Straight { outward, .. } => Some(outward),
                        CrownPath::Round { .. } => None,
                    })
                {
                    positions.push(
                        junction.position + inward
                            - direction_vector(other_outward)
                                * (crown.profile.walk_clear_width_metres + 0.08),
                    );
                }
            }
            for position in positions {
                if !seen_bonds.insert((
                    pair[0].0,
                    pair[1].0,
                    position.x.to_bits(),
                    position.y.to_bits(),
                )) {
                    continue;
                }
                let half = tangent.map_or(Vec2::splat(0.40), |tangent| {
                    if tangent.x.abs() >= tangent.y.abs() {
                        Vec2::new(0.12, crown.profile.thickness_metres * 0.8)
                    } else {
                        Vec2::new(crown.profile.thickness_metres * 0.8, 0.12)
                    }
                });
                let mut bond_height = crown.profile.breastwork_height_metres
                    + crown.profile.merlon_height_metres
                    + crown.profile.coping_height_metres;
                if let Some(tangent) = tangent
                    && let Some(round_owner) = pair.iter().copied().find(|owner| {
                        crowns.iter().any(|other| {
                            other.owner == *owner && matches!(other.path, CrownPath::Round { .. })
                        })
                    })
                    && let Some(node) = geometry
                        .structural_nodes
                        .iter()
                        .find(|node| node.owner == round_owner)
                        .map(|node| node.id)
                {
                    let offset_from_wall = match crown.path {
                        CrownPath::Straight { start, .. } => {
                            (position - start).perp_dot(tangent).abs()
                        }
                        CrownPath::Round { .. } => 0.0,
                    };
                    let (role, transverse, height) = if offset_from_wall > 0.5 {
                        (
                            SolidRole::EdgeGuard,
                            0.12,
                            crown.profile.inner_guard_height_metres,
                        )
                    } else {
                        (
                            SolidRole::Breastwork,
                            crown.profile.thickness_metres,
                            crown.profile.breastwork_height_metres,
                        )
                    };
                    bond_height = height;
                    let size = if tangent.x.abs() >= tangent.y.abs() {
                        Vec3::new(0.16, height, transverse)
                    } else {
                        Vec3::new(transverse, height, 0.16)
                    };
                    let solid_index = geometry.solids.len();
                    let solid = ResolvedSolid {
                        id: ResolvedItemId(
                            (1_u64 << 60) | (u64::from(round_owner.0) << 32) | solid_index as u64,
                        ),
                        owner: round_owner,
                        centre: Vec3::new(
                            position.x,
                            crown.base_height_metres + height * 0.5,
                            position.y,
                        ),
                        size,
                        yaw_radians: 0.0,
                        crossfall_radians: 0.0,
                        longfall_radians: 0.0,
                        role,
                        shape: crate::ResolvedSolidShape::Cuboid,
                        supported_by: vec![node],
                    };
                    let bounds = resolved_axis_bounds(solid.centre, solid.size);
                    geometry.support_interfaces.push(SupportInterface {
                        id: ResolvedItemId((4_u64 << 60) | solid_index as u64),
                        owner: round_owner,
                        node,
                        bounds: ResolvedBounds {
                            min: Vec3::new(bounds.min.x, bounds.min.y - 0.015, bounds.min.z),
                            max: Vec3::new(bounds.max.x, bounds.min.y + 0.015, bounds.max.z),
                        },
                    });
                    geometry.solids.push(solid);
                }
                geometry.junction_bonds.push(JunctionBond {
                    id: ResolvedItemId((6_u64 << 60) | geometry.junction_bonds.len() as u64),
                    owners: pair,
                    bounds: ResolvedBounds {
                        min: Vec3::new(
                            position.x - half.x,
                            crown.base_height_metres - 0.1,
                            position.y - half.y,
                        ),
                        max: Vec3::new(
                            position.x + half.x,
                            crown.base_height_metres + bond_height,
                            position.y + half.y,
                        ),
                    },
                    minimum_interface_area_square_metres: 0.08,
                    maximum_penetration_metres: 0.18,
                });
            }
        }
    }
    geometry
}

fn resolved_axis_bounds(centre: Vec3, size: Vec3) -> ResolvedBounds {
    ResolvedBounds {
        min: centre - size * 0.5,
        max: centre + size * 0.5,
    }
}

fn push_drainage_catchment(
    geometry: &mut ResolvedGeometry,
    crown: &CrownAssembly,
    route: DrainageRoute,
    centre: Vec2,
    tangent: Vec2,
    outward: Vec2,
    length_metres: f32,
) {
    let width_metres = crown.profile.walk_clear_width_metres;
    let channel_width_metres = CROWN_DRAIN_CHANNEL_WIDTH_METRES;
    let slab_width_metres = width_metres - channel_width_metres;
    let outer_elevation_metres = crown.base_height_metres + 0.02;
    let inner_elevation_metres = outer_elevation_metres + 0.06;
    let yaw_radians = -tangent.y.atan2(tangent.x);
    let local_z = Vec2::new(yaw_radians.sin(), yaw_radians.cos());
    let crossfall_sign = local_z.dot(outward).signum();
    let crossfall_radians = crossfall_sign
        * ((inner_elevation_metres - outer_elevation_metres) / slab_width_metres).atan();
    let slab_thickness = 0.12;
    let solid_index = geometry.solids.len();
    let solid_id =
        ResolvedItemId((1_u64 << 60) | (u64::from(crown.owner.0) << 32) | solid_index as u64);
    let support_node = StructuralNodeId(u64::from(crown.owner.0) * 10 + 1);
    let slab_centre = centre - outward * (channel_width_metres * 0.5);
    let solid = ResolvedSolid {
        id: solid_id,
        owner: crown.owner,
        centre: Vec3::new(
            slab_centre.x,
            (inner_elevation_metres + outer_elevation_metres) * 0.5 - slab_thickness * 0.5,
            slab_centre.y,
        ),
        size: Vec3::new(length_metres, slab_thickness, slab_width_metres),
        yaw_radians,
        crossfall_radians,
        longfall_radians: 0.0,
        role: SolidRole::WalkSurface,
        shape: crate::ResolvedSolidShape::Cuboid,
        supported_by: vec![support_node],
    };
    let toe_centre = centre + outward * (width_metres * 0.5 - channel_width_metres * 0.5);
    let inlet = Vec2::new(route.inlet.x, route.inlet.z);
    let outlet_along_metres = (inlet - toe_centre).dot(tangent);
    let outlet_sign = if outlet_along_metres >= 0.0 {
        1.0
    } else {
        -1.0
    };
    let far_toe = toe_centre - tangent * outlet_sign * length_metres * 0.5;
    let channel_points = match crown.path {
        CrownPath::Round {
            centre: tower_centre,
            ..
        } => {
            let near_toe = toe_centre + tangent * outlet_sign * length_metres * 0.5;
            let start_delta = near_toe - tower_centre;
            let end_delta = inlet - tower_centre;
            let start_angle = start_delta.y.atan2(start_delta.x);
            let angle_delta = (end_delta.y.atan2(end_delta.x) - start_angle + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            let steps = (angle_delta.abs() / (std::f32::consts::PI / 48.0))
                .ceil()
                .max(1.0) as usize;
            let gutter_radius = (toe_centre - tower_centre).length();
            let mut points = vec![far_toe, near_toe];
            points.extend((0..=steps).map(|index| {
                let progress = index as f32 / steps as f32;
                let angle = start_angle + angle_delta * progress;
                tower_centre + Vec2::new(angle.cos(), angle.sin()) * gutter_radius
            }));
            if points
                .last()
                .is_none_or(|point| point.distance(inlet) > 0.02)
            {
                points.push(inlet);
            }
            points.dedup_by(|left, right| left.distance(*right) < 0.02);
            points
        }
        CrownPath::Straight { .. } => {
            let near_toe = toe_centre + tangent * outlet_sign * length_metres * 0.5;
            vec![far_toe, near_toe, inlet]
        }
    };
    // Keep the entire open channel floor below the scupper's lower edge. The
    // high end is still below the adjacent toe, so water never has to climb a
    // renderer-only curb before reaching the outlet.
    let channel_drop_metres = 0.018;
    let channel_thickness = 0.05;
    let tangent_extent = tangent.abs() * (length_metres * 0.5);
    let outward_extent = outward.abs() * (width_metres * 0.5);
    let extent = tangent_extent + outward_extent;
    let slab_tangent_extent = tangent.abs() * (length_metres * 0.5);
    let slab_outward_extent = outward.abs() * (slab_width_metres * 0.5);
    let slab_extent = slab_tangent_extent + slab_outward_extent;
    let surface_index = geometry.surfaces.len();
    let surface_id =
        ResolvedItemId((2_u64 << 60) | (u64::from(crown.owner.0) << 32) | surface_index as u64);
    let solid_bottom = solid.centre.y - solid.size.y * 0.5;
    geometry.solids.push(solid);
    let mut channel_ids = Vec::new();
    let channel_count = channel_points.len() - 1;
    for (segment, points) in channel_points.windows(2).enumerate() {
        let channel_index = geometry.solids.len();
        let channel_id =
            ResolvedItemId((1_u64 << 60) | (u64::from(crown.owner.0) << 32) | channel_index as u64);
        let delta = points[1] - points[0];
        let channel_length = delta.length().max(0.04);
        let channel_tangent = delta.normalize_or(tangent);
        let start_height =
            route.inlet.y + channel_drop_metres * (1.0 - segment as f32 / channel_count as f32);
        let end_height = route.inlet.y
            + channel_drop_metres * (1.0 - (segment + 1) as f32 / channel_count as f32);
        let channel = ResolvedSolid {
            id: channel_id,
            owner: crown.owner,
            centre: Vec3::new(
                (points[0].x + points[1].x) * 0.5,
                (start_height + end_height) * 0.5 - channel_thickness * 0.5,
                (points[0].y + points[1].y) * 0.5,
            ),
            size: Vec3::new(channel_length, channel_thickness, channel_width_metres),
            yaw_radians: -channel_tangent.y.atan2(channel_tangent.x),
            crossfall_radians: 0.0,
            // Local +X points from the high end toward the exact scupper inlet.
            longfall_radians: -((start_height - end_height) / channel_length).atan(),
            role: SolidRole::DrainageChannel,
            shape: crate::ResolvedSolidShape::Cuboid,
            supported_by: vec![support_node],
        };
        let channel_bottom = channel.centre.y - channel.size.y * 0.5;
        let channel_extent = channel_tangent.abs() * (channel_length * 0.5)
            + Vec2::new(-channel_tangent.y, channel_tangent.x).abs() * 0.08;
        geometry.support_interfaces.push(SupportInterface {
            id: ResolvedItemId((4_u64 << 60) | channel_index as u64),
            owner: crown.owner,
            node: support_node,
            bounds: ResolvedBounds {
                min: Vec3::new(
                    (points[0].x + points[1].x) * 0.5 - channel_extent.x,
                    channel_bottom - 0.015,
                    (points[0].y + points[1].y) * 0.5 - channel_extent.y,
                ),
                max: Vec3::new(
                    (points[0].x + points[1].x) * 0.5 + channel_extent.x,
                    channel_bottom + 0.015,
                    (points[0].y + points[1].y) * 0.5 + channel_extent.y,
                ),
            },
        });
        geometry.solids.push(channel);
        channel_ids.push(channel_id);
    }
    geometry.surfaces.push(ResolvedSurface {
        id: surface_id,
        owner: crown.owner,
        bounds: ResolvedBounds {
            min: Vec3::new(
                centre.x - extent.x,
                outer_elevation_metres,
                centre.y - extent.y,
            ),
            max: Vec3::new(
                centre.x + extent.x,
                inner_elevation_metres,
                centre.y + extent.y,
            ),
        },
        role: SurfaceRole::Drainage,
        shape: crate::ResolvedSurfaceShape::Planar,
    });
    geometry.support_interfaces.push(SupportInterface {
        id: ResolvedItemId((4_u64 << 60) | solid_index as u64),
        owner: crown.owner,
        node: support_node,
        bounds: ResolvedBounds {
            min: Vec3::new(
                slab_centre.x - slab_extent.x,
                solid_bottom - 0.015,
                slab_centre.y - slab_extent.y,
            ),
            max: Vec3::new(
                slab_centre.x + slab_extent.x,
                solid_bottom + 0.015,
                slab_centre.y + slab_extent.y,
            ),
        },
    });
    geometry.drainage_catchments.push(DrainageCatchment {
        id: ResolvedItemId((7_u64 << 60) | geometry.drainage_catchments.len() as u64),
        owner: crown.owner,
        walk_solid: solid_id,
        toe_channel_solids: channel_ids,
        drainage_surface: surface_id,
        outlet_route: route.id,
        centre: Vec3::new(
            centre.x,
            (inner_elevation_metres + outer_elevation_metres) * 0.5,
            centre.y,
        ),
        tangent,
        outward,
        length_metres,
        width_metres,
        inner_elevation_metres,
        outer_elevation_metres,
        outlet_along_metres,
    });
}

fn resolved_solid_contains_point(solid: &ResolvedSolid, point: Vec3, tolerance: f32) -> bool {
    let relative = point - solid.centre;
    let (sine, cosine) = solid.yaw_radians.sin_cos();
    let local = Vec3::new(
        relative.x * cosine - relative.z * sine,
        relative.y,
        relative.x * sine + relative.z * cosine,
    );
    let half = solid.size * 0.5 + Vec3::splat(tolerance);
    local.abs().cmple(half).all()
}

fn linear_walk_bounds_for_geometry(walk: WallWalk) -> ResolvedBounds {
    let WallWalk::Linear {
        start,
        end,
        elevation_metres,
        width_metres,
        outward,
    } = walk
    else {
        unreachable!()
    };
    let inward = -direction_vector(outward) * width_metres;
    let min = start.min(end).min(start + inward).min(end + inward);
    let max = start.max(end).max(start + inward).max(end + inward);
    ResolvedBounds {
        min: Vec3::new(min.x, elevation_metres - 0.08, min.y),
        max: Vec3::new(max.x, elevation_metres, max.y),
    }
}

fn derive_wall_walks(
    program: &BuildingProgram,
    battlements: &[BattlementRun],
    towers: &[RoundTower],
) -> Vec<WallWalk> {
    let mut walks = battlements
        .iter()
        .filter(|run| run.kind != BattlementKind::Breteche)
        .map(|run| WallWalk::Linear {
            start: run.start,
            end: run.end,
            elevation_metres: run.base_height_metres,
            width_metres: 1.25,
            outward: run.outward,
        })
        .chain(
            towers
                .iter()
                .filter(|tower| tower.battlement.is_some())
                .map(|tower| WallWalk::Round {
                    centre: tower.centre_metres(),
                    elevation_metres: tower.wall_height_metres,
                    outer_radius_metres: tower.radius_metres() - 0.08,
                    stairwell_radius_metres: 0.62,
                }),
        )
        .collect::<Vec<_>>();
    if program.archetype == BuildingArchetype::WalledKeep {
        let (width, depth) = program.footprint.dimensions();
        let size = Vec2::new(
            f32::from(width) * CELL_SIZE_METRES,
            f32::from(depth) * CELL_SIZE_METRES,
        );
        walks.push(WallWalk::RectangularDeck {
            centre: size * 0.5,
            size,
            elevation_metres: program.storeys.len() as f32 * program.storey_height_metres,
            stairwell_centre: size * 0.5,
            stairwell_size: Vec2::splat(1.6),
        });
    }
    walks
}

fn derive_defensive_junctions(walks: &[WallWalk]) -> Vec<DefensiveJunction> {
    let mut junctions = Vec::new();
    for walk_a in 0..walks.len() {
        for walk_b in (walk_a + 1)..walks.len() {
            let Some(centre) = walk_junction_centre(walks[walk_a], walks[walk_b]) else {
                continue;
            };
            let elevation_delta =
                (walk_elevation(walks[walk_a]) - walk_elevation(walks[walk_b])).abs();
            let kind = if elevation_delta <= 0.2 {
                DefensiveJunctionKind::LevelLanding
            } else {
                DefensiveJunctionKind::Steps {
                    riser_count: (elevation_delta / 0.18).ceil() as u8,
                }
            };
            junctions.push(DefensiveJunction {
                walk_a,
                walk_b,
                centre,
                width_metres: 1.0,
                clear_height_metres: 2.1,
                kind,
            });
        }
    }
    junctions
}

fn derive_defensive_circuits(
    program: &BuildingProgram,
    walks: &[WallWalk],
) -> Vec<DefensiveCircuit> {
    if walks.is_empty() {
        return Vec::new();
    }
    if program.archetype != BuildingArchetype::WalledKeep {
        return vec![DefensiveCircuit {
            label: "main fighting circuit".to_owned(),
            walks: (0..walks.len()).collect(),
        }];
    }
    let dimensions = Vec2::new(
        f32::from(program.footprint.dimensions().0) * CELL_SIZE_METRES,
        f32::from(program.footprint.dimensions().1) * CELL_SIZE_METRES,
    );
    let is_outer = |walk: WallWalk| match walk {
        WallWalk::Linear { start, end, .. } => {
            start.x < -0.01
                || start.y < -0.01
                || end.x > dimensions.x + 0.01
                || end.y > dimensions.y + 0.01
        }
        WallWalk::Round { centre, .. } => {
            centre.x < -0.01
                || centre.y < -0.01
                || centre.x > dimensions.x + 0.01
                || centre.y > dimensions.y + 0.01
        }
        WallWalk::RectangularDeck { .. } => false,
    };
    let (outer, inner): (Vec<_>, Vec<_>) = walks
        .iter()
        .copied()
        .enumerate()
        .partition(|(_, walk)| is_outer(*walk));
    vec![
        DefensiveCircuit {
            label: "outer curtain circuit".to_owned(),
            walks: outer.into_iter().map(|(index, _)| index).collect(),
        },
        DefensiveCircuit {
            label: "inner keep circuit".to_owned(),
            walks: inner.into_iter().map(|(index, _)| index).collect(),
        },
    ]
}

fn derive_tower_portals(
    program: &BuildingProgram,
    towers: &[RoundTower],
    walks: &[WallWalk],
    junctions: &[DefensiveJunction],
) -> Vec<TowerPortal> {
    let dimensions = Vec2::new(
        f32::from(program.footprint.dimensions().0) * CELL_SIZE_METRES,
        f32::from(program.footprint.dimensions().1) * CELL_SIZE_METRES,
    );
    let protected_centre = dimensions * 0.5;
    let mut portals = towers
        .iter()
        .enumerate()
        .map(|(tower_index, tower)| TowerPortal {
            tower_index,
            facing: cardinal_direction(protected_centre - tower.centre_metres()),
            sill_elevation_metres: 0.0,
            width_metres: 1.05,
            clear_height_metres: 2.15,
            kind: TowerPortalKind::GroundStairEntrance,
        })
        .collect::<Vec<_>>();
    for junction in junctions {
        let pair = [junction.walk_a, junction.walk_b];
        let Some(&linear_index) = pair
            .iter()
            .find(|&&index| matches!(walks.get(index), Some(WallWalk::Linear { .. })))
        else {
            continue;
        };
        let Some((round_index, tower_centre, elevation)) = pair.iter().find_map(|&index| {
            let WallWalk::Round {
                centre,
                elevation_metres,
                ..
            } = *walks.get(index)?
            else {
                return None;
            };
            Some((index, centre, elevation_metres))
        }) else {
            continue;
        };
        let Some(tower_index) = towers
            .iter()
            .position(|tower| (tower.centre_metres() - tower_centre).length_squared() < 0.001)
        else {
            continue;
        };
        let WallWalk::Linear { start, end, .. } = walks[linear_index] else {
            unreachable!()
        };
        let along =
            if (start - tower_centre).length_squared() < (end - tower_centre).length_squared() {
                end - start
            } else {
                start - end
            };
        portals.push(TowerPortal {
            tower_index,
            facing: cardinal_direction(along),
            sill_elevation_metres: elevation - 0.2,
            width_metres: junction.width_metres,
            clear_height_metres: junction.clear_height_metres,
            kind: TowerPortalKind::WallWalkJunction {
                walk_index: linear_index,
            },
        });
        let _ = round_index;
    }
    portals
}

fn cardinal_direction(vector: Vec2) -> Direction {
    if vector.x.abs() >= vector.y.abs() {
        if vector.x >= 0.0 {
            Direction::East
        } else {
            Direction::West
        }
    } else if vector.y >= 0.0 {
        Direction::North
    } else {
        Direction::South
    }
}

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

fn direction_vector(direction: Direction) -> Vec2 {
    match direction {
        Direction::North => Vec2::Y,
        Direction::East => Vec2::X,
        Direction::South => -Vec2::Y,
        Direction::West => -Vec2::X,
    }
}

fn walk_elevation(walk: WallWalk) -> f32 {
    match walk {
        WallWalk::Linear {
            elevation_metres, ..
        }
        | WallWalk::Round {
            elevation_metres, ..
        }
        | WallWalk::RectangularDeck {
            elevation_metres, ..
        } => elevation_metres,
    }
}

fn walk_junction_centre(a: WallWalk, b: WallWalk) -> Option<Vec2> {
    match (a, b) {
        (
            WallWalk::Linear {
                start,
                end,
                width_metres,
                ..
            },
            WallWalk::Round {
                centre,
                outer_radius_metres,
                ..
            },
        )
        | (
            WallWalk::Round {
                centre,
                outer_radius_metres,
                ..
            },
            WallWalk::Linear {
                start,
                end,
                width_metres,
                ..
            },
        ) => {
            let delta = end - start;
            let t = if delta.length_squared() < 0.001 {
                0.0
            } else {
                ((centre - start).dot(delta) / delta.length_squared()).clamp(0.0, 1.0)
            };
            let nearest = start + delta * t;
            ((nearest - centre).length() <= outer_radius_metres + width_metres * 0.5)
                .then_some(nearest)
        }
        (
            WallWalk::Linear {
                start: a0,
                end: a1,
                width_metres: aw,
                ..
            },
            WallWalk::Linear {
                start: b0,
                end: b1,
                width_metres: bw,
                ..
            },
        ) => [a0, a1]
            .into_iter()
            .find(|point| distance_to_segment(*point, b0, b1) <= (aw + bw) * 0.5),
        (
            WallWalk::Linear {
                start,
                end,
                width_metres,
                ..
            },
            WallWalk::RectangularDeck { centre, size, .. },
        )
        | (
            WallWalk::RectangularDeck { centre, size, .. },
            WallWalk::Linear {
                start,
                end,
                width_metres,
                ..
            },
        ) => {
            let half = size * 0.5 + Vec2::splat(width_metres * 0.5);
            [start, end].into_iter().find(|point| {
                point.x >= centre.x - half.x
                    && point.x <= centre.x + half.x
                    && point.y >= centre.y - half.y
                    && point.y <= centre.y + half.y
            })
        }
        _ => None,
    }
}

fn distance_to_segment(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let delta = end - start;
    if delta.length_squared() < 0.001 {
        return (point - start).length();
    }
    let t = ((point - start).dot(delta) / delta.length_squared()).clamp(0.0, 1.0);
    (point - (start + delta * t)).length()
}

fn derive_curtain_walls(program: &BuildingProgram) -> Vec<CurtainWallRun> {
    if program.archetype != BuildingArchetype::WalledKeep {
        return Vec::new();
    }
    let (width, depth) = program.footprint.dimensions();
    let width = f32::from(width) * CELL_SIZE_METRES;
    let depth = f32::from(depth) * CELL_SIZE_METRES;
    let margin = 9.0;
    let min = Vec2::splat(-margin);
    let max = Vec2::new(width + margin, depth + margin);
    let wall = |start, end, outward, gate_width_metres| CurtainWallRun {
        start,
        end,
        height_metres: 6.0,
        // 1.2 m is an inferred prototype minimum for a deliberately practical
        // early-artillery profile, not a universal historical threshold.
        thickness_metres: 1.2,
        outward,
        gate_width_metres,
        gate_height_metres: 3.6,
    };
    vec![
        wall(min, Vec2::new(max.x, min.y), Direction::South, Some(3.2)),
        wall(Vec2::new(max.x, min.y), max, Direction::East, None),
        wall(Vec2::new(min.x, max.y), max, Direction::North, None),
        wall(min, Vec2::new(min.x, max.y), Direction::West, None),
    ]
}

fn derive_bartizans(program: &BuildingProgram) -> Vec<Bartizan> {
    let (width, depth) = program.footprint.dimensions();
    let width = f32::from(width) * CELL_SIZE_METRES;
    let depth = f32::from(depth) * CELL_SIZE_METRES;
    let top = program.storeys.len() as f32 * program.storey_height_metres;
    match program.archetype {
        BuildingArchetype::CastleGatehouse if program.seed % 1_000 == 203 => vec![
            Bartizan {
                // Keep the unroofed bartizan on its own grounded buttress bay,
                // beyond the resolved south gate-tower radius. It remains a
                // localized threatened-face work instead of overlapping the
                // newly authoritative radial tower shell.
                centre: Vec2::new(width + 0.4, depth * 0.44),
                base_height_metres: top,
                radius_metres: 0.85,
                height_metres: 2.0,
                roofed: false,
            },
            Bartizan {
                centre: Vec2::new(width + 0.4, depth * 0.8),
                base_height_metres: top,
                radius_metres: 0.85,
                height_metres: 2.0,
                roofed: true,
            },
        ],
        BuildingArchetype::CastleGatehouse | BuildingArchetype::CourtyardCastle => Vec::new(),
        _ => Vec::new(),
    }
}

fn projected_solid(
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    centre: Vec3,
    size: Vec3,
    yaw_radians: f32,
    role: SolidRole,
    supported_by: Vec<StructuralNodeId>,
) -> ResolvedItemId {
    let index = geometry.solids.len();
    let id = ResolvedItemId((1_u64 << 60) | (u64::from(owner.0) << 32) | index as u64);
    let solid = ResolvedSolid {
        id,
        owner,
        centre,
        size,
        yaw_radians,
        crossfall_radians: 0.0,
        longfall_radians: 0.0,
        role,
        shape: crate::ResolvedSolidShape::Cuboid,
        supported_by: supported_by.clone(),
    };
    let bottom = centre.y - size.y * 0.5;
    geometry.support_interfaces.push(SupportInterface {
        id: ResolvedItemId((4_u64 << 60) | index as u64),
        owner,
        node: supported_by[0],
        bounds: ResolvedBounds {
            min: Vec3::new(
                centre.x - size.x * 0.5,
                bottom - 0.015,
                centre.z - size.z * 0.5,
            ),
            max: Vec3::new(
                centre.x + size.x * 0.5,
                bottom + 0.015,
                centre.z + size.z * 0.5,
            ),
        },
    });
    geometry.solids.push(solid);
    id
}

fn projected_void(
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    bounds: ResolvedBounds,
    role: VoidRole,
) -> ResolvedItemId {
    let index = geometry.voids.len();
    let id = ResolvedItemId((3_u64 << 60) | (u64::from(owner.0) << 32) | index as u64);
    geometry.voids.push(ResolvedVoid {
        id,
        owner,
        bounds,
        role,
        shape: crate::ResolvedVoidShape::Box,
        subtracts_from: owner,
    });
    id
}

fn projected_surface(
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    bounds: ResolvedBounds,
    role: SurfaceRole,
) -> ResolvedItemId {
    let index = geometry.surfaces.len();
    let id = ResolvedItemId((2_u64 << 60) | (u64::from(owner.0) << 32) | index as u64);
    geometry.surfaces.push(ResolvedSurface {
        id,
        owner,
        bounds,
        role,
        shape: crate::ResolvedSurfaceShape::Planar,
    });
    id
}

fn projected_edge_drain(
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    inlet: Vec3,
    direction: Vec2,
) -> ResolvedItemId {
    let far = inlet + Vec3::new(direction.x * 0.12, -0.02, direction.y * 0.12);
    let lateral = Vec3::new(direction.y.abs() * 0.01, 0.0, direction.x.abs() * 0.01);
    let outlet_void = projected_void(
        geometry,
        owner,
        ResolvedBounds {
            min: inlet.min(far) - lateral - Vec3::Y * 0.045,
            max: inlet.max(far) + lateral + Vec3::Y * 0.045,
        },
        VoidRole::Drain,
    );
    let id = ResolvedItemId((5_u64 << 60) | geometry.drainage_routes.len() as u64);
    geometry.drainage_routes.push(DrainageRoute {
        id,
        owner,
        outlet_void,
        inlet,
        outlet: far + Vec3::new(direction.x * 0.25, -0.08, direction.y * 0.25),
    });
    id
}

/// Resolves a mono-pitch defense roof into a physical catchment, a lowered
/// eave channel and a named drip outlet. The high host edge receives separate
/// flashing so no valley can be trapped between roof and source masonry.
fn resolve_linear_roof_weathering(
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    roof_id: ResolvedItemId,
    midpoint: Vec2,
    tangent: Vec2,
    outward: Vec2,
    length: f32,
    depth: f32,
    yaw: f32,
    support: StructuralNodeId,
) -> (ResolvedItemId, Vec<ResolvedItemId>) {
    let roof = geometry
        .solids
        .iter()
        .find(|solid| solid.id == roof_id)
        .expect("roof catchment solid")
        .clone();
    let local_positive_z = Vec2::new(yaw.sin(), yaw.cos());
    let crossfall = 0.12 * outward.dot(local_positive_z).signum();
    geometry
        .solids
        .iter_mut()
        .find(|solid| solid.id == roof_id)
        .expect("roof catchment solid")
        .crossfall_radians = crossfall;
    let inner_edge = midpoint - outward * depth * 0.5;
    let flashing = projected_solid(
        geometry,
        owner,
        Vec3::new(
            inner_edge.x - outward.x * 0.035,
            roof.centre.y + 0.13,
            inner_edge.y - outward.y * 0.035,
        ),
        Vec3::new(length + 0.18, 0.26, 0.08),
        yaw,
        SolidRole::RoofFlashing,
        vec![support],
    );
    let roof_half_drop = crossfall.abs().tan() * depth * 0.5;
    let toe_elevation = roof.centre.y - roof_half_drop;
    let channel_length = length + 0.24;
    let channel_centre_plan = midpoint + outward * (depth * 0.5 + 0.06) - tangent * 0.055;
    let channel = projected_solid(
        geometry,
        owner,
        Vec3::new(
            channel_centre_plan.x,
            toe_elevation - 0.045,
            channel_centre_plan.y,
        ),
        Vec3::new(channel_length - 0.11, 0.06, 0.12),
        yaw,
        SolidRole::DrainageFloor,
        vec![support],
    );
    geometry
        .solids
        .iter_mut()
        .find(|solid| solid.id == channel)
        .expect("roof eave channel")
        .longfall_radians = -0.018;
    let inlet_plan = channel_centre_plan + tangent * ((channel_length - 0.11) * 0.5 + 0.018);
    let route = projected_edge_drain(
        geometry,
        owner,
        Vec3::new(inlet_plan.x, toe_elevation - 0.015, inlet_plan.y),
        outward,
    );
    let surface = projected_surface(
        geometry,
        owner,
        ResolvedBounds {
            min: roof.centre - roof.size * 0.5,
            max: roof.centre + roof.size * 0.5,
        },
        SurfaceRole::Drainage,
    );
    let catchment = ResolvedItemId((7_u64 << 60) | geometry.drainage_catchments.len() as u64);
    geometry.drainage_catchments.push(DrainageCatchment {
        id: catchment,
        owner,
        walk_solid: roof_id,
        toe_channel_solids: vec![channel],
        drainage_surface: surface,
        outlet_route: route,
        centre: roof.centre,
        tangent,
        outward,
        length_metres: length,
        width_metres: depth,
        inner_elevation_metres: roof.centre.y + roof_half_drop,
        outer_elevation_metres: toe_elevation,
        outlet_along_metres: (channel_length - 0.11) * 0.5,
    });
    (catchment, vec![roof_id, channel, flashing])
}

fn resolve_linear_coping_weathering(
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    centre: Vec2,
    tangent: Vec2,
    outward: Vec2,
    length: f32,
    elevation: f32,
    yaw: f32,
    support: StructuralNodeId,
) -> (ResolvedItemId, Vec<ResolvedItemId>) {
    let coping = projected_solid(
        geometry,
        owner,
        Vec3::new(
            centre.x + outward.x * 0.035,
            elevation,
            centre.y + outward.y * 0.035,
        ),
        Vec3::new(length + 0.12, 0.12, 0.32),
        yaw,
        SolidRole::Coping,
        vec![support],
    );
    let local_positive_z = Vec2::new(yaw.sin(), yaw.cos());
    let crossfall = 0.07 * outward.dot(local_positive_z).signum();
    geometry
        .solids
        .iter_mut()
        .find(|solid| solid.id == coping)
        .expect("projected coping")
        .crossfall_radians = crossfall;
    let toe = elevation - 0.06 - crossfall.abs().tan() * 0.16;
    let inlet_plan = centre + tangent * (length * 0.5 - 0.06) + outward * 0.2;
    let route = projected_edge_drain(
        geometry,
        owner,
        Vec3::new(inlet_plan.x, toe - 0.06, inlet_plan.y),
        outward,
    );
    let surface = projected_surface(
        geometry,
        owner,
        ResolvedBounds {
            min: Vec3::new(centre.x, toe, centre.y)
                - Vec3::new(
                    tangent.x.abs() * length * 0.5 + outward.x.abs() * 0.16,
                    0.0,
                    tangent.y.abs() * length * 0.5 + outward.y.abs() * 0.16,
                ),
            max: Vec3::new(centre.x, elevation + 0.06, centre.y)
                + Vec3::new(
                    tangent.x.abs() * length * 0.5 + outward.x.abs() * 0.16,
                    0.0,
                    tangent.y.abs() * length * 0.5 + outward.y.abs() * 0.16,
                ),
        },
        SurfaceRole::Drainage,
    );
    let catchment = ResolvedItemId((7_u64 << 60) | geometry.drainage_catchments.len() as u64);
    geometry.drainage_catchments.push(DrainageCatchment {
        id: catchment,
        owner,
        walk_solid: coping,
        toe_channel_solids: Vec::new(),
        drainage_surface: surface,
        outlet_route: route,
        centre: Vec3::new(centre.x, elevation, centre.y),
        tangent,
        outward,
        length_metres: length,
        width_metres: 0.32,
        inner_elevation_metres: elevation + crossfall.abs().tan() * 0.16,
        outer_elevation_metres: toe,
        outlet_along_metres: length * 0.5 - 0.06,
    });
    (catchment, vec![coping])
}

struct LinearDefenseHost {
    owner: GeometryOwnerId,
    bearing: StructuralNodeId,
    walls: Vec<ResolvedItemId>,
    buttresses: Vec<ResolvedItemId>,
    sources: Vec<ProjectedDefenseHostWallSource>,
    top_elevation_metres: f32,
    topology: ProjectedDefenseHostTopology,
    walk: ResolvedItemId,
    portal: Option<ResolvedItemId>,
    sockets: Vec<ResolvedItemId>,
}

/// Resolves the masonry that a projected defense actually cuts and bears on.
/// Dimensions here are project gates for coarse traversal/rendering, not a
/// claim that every historical curtain used this exact section.
fn resolve_linear_defense_host(
    geometry: &mut ResolvedGeometry,
    storeys: &[StoreyPlan],
    source_index: usize,
    run: BattlementRun,
    socket_count: Option<usize>,
    needs_portal: bool,
) -> LinearDefenseHost {
    let owner = GeometryOwnerId(10_000 + source_index as u32);
    let bearing = StructuralNodeId(900_000 + source_index as u64 * 100);
    let tangent = (run.end - run.start).normalize_or_zero();
    let outward = direction_vector(run.outward);
    let midpoint = (run.start + run.end) * 0.5;
    let length = run.start.distance(run.end);
    let yaw = -tangent.y.atan2(tangent.x);
    geometry.structural_nodes.push(StructuralNode {
        id: bearing,
        owner,
        kind: StructuralNodeKind::WallBearing,
        position: Vec3::new(midpoint.x, 0.0, midpoint.y),
        supported_by: Vec::new(),
        grounded: true,
    });
    let top_storey = storeys.last().expect("projected defense host storey");
    let wall_top = f32::from(top_storey.level + 1)
        * (run.base_height_metres / f32::from(top_storey.level + 1));
    let wall_bottom = wall_top - run.base_height_metres / f32::from(top_storey.level + 1);
    let wall_depth = 0.18;
    let source_walls = top_storey
        .walls
        .iter()
        .enumerate()
        .filter(|(_, wall)| {
            if !wall.exterior() || wall.direction != run.outward {
                return false;
            }
            let offset = wall.centre() - midpoint;
            offset.dot(outward).abs() <= 0.12
                && offset.dot(tangent).abs() <= length * 0.5 + CELL_SIZE_METRES * 0.51
        })
        .map(|(wall_index, wall)| (wall_index, *wall))
        .collect::<Vec<_>>();
    assert!(
        !source_walls.is_empty(),
        "projected defense must bind real source wall cells"
    );

    let mut cuts = Vec::<(f32, f32, f32, f32, VoidRole)>::new();
    let mut portal = None;
    if needs_portal {
        let width = 0.9;
        let bottom = wall_top - 0.14;
        let top = wall_top + 2.0;
        cuts.push((
            -width * 0.5,
            width * 0.5,
            bottom,
            top,
            VoidRole::AccessPortal,
        ));
    }
    if let Some(count) = socket_count {
        let bay = length / count as f32;
        for index in 0..count {
            let centre = -length * 0.5 + (index as f32 + 0.5) * bay;
            cuts.push((
                centre - 0.09,
                centre + 0.09,
                wall_top - 0.52,
                wall_top - 0.28,
                VoidRole::BeamSocket,
            ));
        }
    }
    let source_average = source_walls
        .iter()
        .map(|(_, wall)| wall.centre())
        .fold(Vec2::ZERO, |sum, centre| sum + centre)
        / source_walls.len() as f32;
    let host_line = midpoint + outward * (source_average - midpoint).dot(outward);
    for (from, to, bottom, top, role) in &cuts {
        let centre = host_line + tangent * ((*from + *to) * 0.5);
        let id = projected_void(
            geometry,
            owner,
            ResolvedBounds {
                min: Vec3::new(
                    centre.x
                        - tangent.x.abs() * (*to - *from) * 0.5
                        - outward.x.abs() * wall_depth * 0.5,
                    *bottom,
                    centre.y
                        - tangent.y.abs() * (*to - *from) * 0.5
                        - outward.y.abs() * wall_depth * 0.5,
                ),
                max: Vec3::new(
                    centre.x
                        + tangent.x.abs() * (*to - *from) * 0.5
                        + outward.x.abs() * wall_depth * 0.5,
                    *top,
                    centre.y
                        + tangent.y.abs() * (*to - *from) * 0.5
                        + outward.y.abs() * wall_depth * 0.5,
                ),
            },
            *role,
        );
        match role {
            VoidRole::AccessPortal => portal = Some(id),
            VoidRole::BeamSocket => {}
            _ => unreachable!(),
        }
    }
    let sockets = geometry
        .voids
        .iter()
        .filter(|void| void.owner == owner && void.role == VoidRole::BeamSocket)
        .map(|void| void.id)
        .collect::<Vec<_>>();
    let mut walls = Vec::new();
    for (_, wall) in &source_walls {
        let along_centre = (wall.centre() - midpoint).dot(tangent);
        let segment_min = (along_centre - CELL_SIZE_METRES * 0.5).max(-length * 0.5);
        let segment_max = (along_centre + CELL_SIZE_METRES * 0.5).min(length * 0.5);
        let mut along_cuts = vec![segment_min, segment_max];
        let mut height_cuts = vec![wall_bottom, wall_top];
        for (from, to, bottom, top, _) in &cuts {
            if *to > segment_min && *from < segment_max {
                along_cuts.extend([from.max(segment_min), to.min(segment_max)]);
                height_cuts.extend([bottom.max(wall_bottom), top.min(wall_top)]);
            }
        }
        along_cuts.sort_by(f32::total_cmp);
        along_cuts.dedup_by(|a, b| (*a - *b).abs() < 0.001);
        height_cuts.sort_by(f32::total_cmp);
        height_cuts.dedup_by(|a, b| (*a - *b).abs() < 0.001);
        for along in along_cuts.windows(2) {
            for height in height_cuts.windows(2) {
                let ac = (along[0] + along[1]) * 0.5;
                let hc = (height[0] + height[1]) * 0.5;
                if cuts.iter().any(|(from, to, bottom, top, _)| {
                    ac > *from && ac < *to && hc > *bottom && hc < *top
                }) {
                    continue;
                }
                let centre = midpoint + tangent * ac;
                walls.push(projected_solid(
                    geometry,
                    owner,
                    Vec3::new(centre.x, hc, centre.y),
                    Vec3::new(along[1] - along[0], height[1] - height[0], wall_depth),
                    yaw,
                    SolidRole::DefenseHostWall,
                    vec![bearing],
                ));
            }
        }
    }
    let walk_centre = midpoint - outward * 0.84;
    let walk = projected_solid(
        geometry,
        owner,
        Vec3::new(walk_centre.x, run.base_height_metres - 0.07, walk_centre.y),
        Vec3::new(length, 0.14, 1.0),
        yaw,
        SolidRole::CircuitWalk,
        vec![bearing],
    );
    LinearDefenseHost {
        owner,
        bearing,
        walls,
        buttresses: Vec::new(),
        sources: source_walls
            .into_iter()
            .map(|(wall_index, _)| ProjectedDefenseHostWallSource {
                storey_level: top_storey.level,
                wall_index,
            })
            .collect(),
        top_elevation_metres: wall_top,
        topology: ProjectedDefenseHostTopology::LinearFace,
        walk,
        portal,
        sockets,
    }
}

fn resolve_projected_defenses(
    program: &BuildingProgram,
    storeys: &[StoreyPlan],
    battlements: &[BattlementRun],
    bartizans: &[Bartizan],
    geometry: &mut ResolvedGeometry,
) -> Vec<ProjectedDefenseAssembly> {
    let mut assemblies = Vec::new();
    for (source_index, run) in battlements.iter().copied().enumerate() {
        let (kind, material, phase, deployment, tactical_target, roofed) = match run.kind {
            BattlementKind::Machicolated => (
                ProjectedDefenseKind::Machicolation,
                ProjectedDefenseMaterial::Masonry,
                ProjectedDefensePhase::PermanentMainWork,
                ProjectedDefenseDeployment::Permanent,
                ProjectedDefenseTarget::GateApproach,
                false,
            ),
            BattlementKind::Breteche => (
                ProjectedDefenseKind::Breteche,
                ProjectedDefenseMaterial::Masonry,
                ProjectedDefensePhase::PermanentMainWork,
                ProjectedDefenseDeployment::Permanent,
                ProjectedDefenseTarget::ThreatenedWallFoot,
                true,
            ),
            BattlementKind::OpenHoarding => (
                ProjectedDefenseKind::Hoarding,
                ProjectedDefenseMaterial::Timber,
                ProjectedDefensePhase::TemporaryCampaignWork,
                ProjectedDefenseDeployment::SocketsOnly,
                ProjectedDefenseTarget::CampaignSiegeFront,
                false,
            ),
            BattlementKind::RoofedHoarding => (
                ProjectedDefenseKind::Hoarding,
                ProjectedDefenseMaterial::Timber,
                ProjectedDefensePhase::TemporaryCampaignWork,
                ProjectedDefenseDeployment::Deployed,
                ProjectedDefenseTarget::CampaignSiegeFront,
                true,
            ),
            _ => continue,
        };
        let owner = GeometryOwnerId(1_000 + source_index as u32);
        let tangent = (run.end - run.start).normalize_or_zero();
        let outward = direction_vector(run.outward);
        let length = run.start.distance(run.end);
        let yaw = -tangent.y.atan2(tangent.x);
        let socket_count = (material == ProjectedDefenseMaterial::Timber)
            .then(|| (length / 1.1).ceil().max(2.0) as usize);
        let host = resolve_linear_defense_host(
            geometry,
            storeys,
            source_index,
            run,
            socket_count,
            deployment != ProjectedDefenseDeployment::SocketsOnly,
        );
        let wall_node = host.bearing;
        let bond_id = ResolvedItemId((6_u64 << 60) | source_index as u64);
        let midpoint = (run.start + run.end) * 0.5;
        geometry.junction_bonds.push(JunctionBond {
            id: bond_id,
            owners: [host.owner, owner],
            bounds: ResolvedBounds {
                min: Vec3::new(
                    midpoint.x - tangent.x.abs() * (length * 0.5 + 0.12) - outward.x.abs() * 0.65,
                    run.base_height_metres - 0.65,
                    midpoint.y - tangent.y.abs() * (length * 0.5 + 0.12) - outward.y.abs() * 0.65,
                ),
                max: Vec3::new(
                    midpoint.x + tangent.x.abs() * (length * 0.5 + 0.12) + outward.x.abs() * 0.65,
                    run.base_height_metres + 2.6,
                    midpoint.y + tangent.y.abs() * (length * 0.5 + 0.12) + outward.y.abs() * 0.65,
                ),
            },
            minimum_interface_area_square_metres: 0.08,
            maximum_penetration_metres: 0.18,
        });
        if deployment == ProjectedDefenseDeployment::SocketsOnly {
            assemblies.push(ProjectedDefenseAssembly {
                owner,
                host_owner: host.owner,
                host_wall_solids: host.walls,
                host_buttress_solids: host.buttresses,
                host_source_walls: host.sources,
                host_top_elevation_metres: host.top_elevation_metres,
                host_topology: host.topology,
                host_walk_solid: host.walk,
                host_portal_void: None,
                host_bond: None,
                beam_socket_voids: host.sockets,
                socket_joists: Vec::new(),
                kind,
                material,
                phase,
                deployment,
                tactical_target,
                path: ProjectedDefensePath::Linear {
                    start: run.start,
                    end: run.end,
                    outward: run.outward,
                },
                floor_elevation_metres: run.base_height_metres,
                clear_width_metres: 0.0,
                clear_height_metres: 0.0,
                projection_metres: 0.0,
                breastwork_height_metres: 0.0,
                roofed,
                floor_solids: Vec::new(),
                throat_voids: Vec::new(),
                access_portal: None,
                access_landing: None,
                firing_apertures: Vec::new(),
                support_nodes: Vec::new(),
                drain_route: None,
                drainage_catchments: Vec::new(),
                weather_catchments: Vec::new(),
                weathering_solids: Vec::new(),
                roof_support_solids: Vec::new(),
                roof_bearing_node: None,
            });
            geometry.junction_bonds.pop();
            continue;
        }
        let projection = if material == ProjectedDefenseMaterial::Timber {
            1.15
        } else {
            1.35
        };
        let inner_walk = 0.9;
        let throat_depth = projection - inner_walk - 0.14;
        let floor_node = StructuralNodeId(wall_node.0 + 90);
        let bay_count = (length / 1.05).floor().max(2.0) as usize;
        let mut support_nodes = Vec::new();
        for index in 0..=bay_count {
            let progress = index as f32 / bay_count as f32;
            let mut anchor = run.start.lerp(run.end, progress);
            if index == 0 {
                anchor += tangent * 0.10;
            } else if index == bay_count {
                anchor -= tangent * 0.10;
            }
            let node = StructuralNodeId(wall_node.0 + 1 + index as u64);
            support_nodes.push(node);
            geometry.structural_nodes.push(StructuralNode {
                id: node,
                owner,
                kind: if material == ProjectedDefenseMaterial::Timber {
                    StructuralNodeKind::GalleryFrame
                } else {
                    StructuralNodeKind::ProjectionCorbel
                },
                position: Vec3::new(
                    anchor.x + outward.x * projection * 0.5,
                    run.base_height_metres - 0.42,
                    anchor.y + outward.y * projection * 0.5,
                ),
                supported_by: vec![wall_node],
                grounded: false,
            });
            projected_solid(
                geometry,
                owner,
                Vec3::new(
                    anchor.x + outward.x * projection * 0.42,
                    run.base_height_metres - 0.38,
                    anchor.y + outward.y * projection * 0.42,
                ),
                Vec3::new(0.16, 0.34, projection * 0.84),
                yaw,
                if material == ProjectedDefenseMaterial::Timber {
                    SolidRole::FrameMember
                } else {
                    SolidRole::ProjectionSupport
                },
                vec![wall_node],
            );
        }
        geometry.structural_nodes.push(StructuralNode {
            id: floor_node,
            owner,
            kind: StructuralNodeKind::GalleryFrame,
            position: Vec3::new(midpoint.x, run.base_height_metres, midpoint.y),
            supported_by: support_nodes.clone(),
            grounded: false,
        });
        for node in &support_nodes {
            let position = geometry
                .structural_nodes
                .iter()
                .find(|candidate| candidate.id == *node)
                .expect("projected support node")
                .position;
            let tangent_extent = tangent.abs() * 0.08;
            let outward_extent = outward.abs() * (projection * 0.5);
            let extent = tangent_extent + outward_extent;
            geometry.support_interfaces.push(SupportInterface {
                id: ResolvedItemId((4_u64 << 60) | geometry.support_interfaces.len() as u64),
                owner,
                node: *node,
                bounds: ResolvedBounds {
                    min: Vec3::new(
                        position.x - extent.x,
                        run.base_height_metres - 0.09,
                        position.z - extent.y,
                    ),
                    max: Vec3::new(
                        position.x + extent.x,
                        run.base_height_metres - 0.06,
                        position.z + extent.y,
                    ),
                },
            });
        }
        let mut socket_joists = Vec::new();
        if material == ProjectedDefenseMaterial::Timber {
            for socket in &host.sockets {
                let bounds = geometry
                    .voids
                    .iter()
                    .find(|void| void.id == *socket)
                    .expect("host beam socket")
                    .bounds;
                let socket_centre = (bounds.min + bounds.max) * 0.5;
                let centre = Vec2::new(socket_centre.x, socket_centre.z) + outward * (0.52 - 0.17);
                let joist = projected_solid(
                    geometry,
                    owner,
                    Vec3::new(centre.x, socket_centre.y, centre.y),
                    Vec3::new(0.16, 0.18, 1.04),
                    yaw,
                    SolidRole::BeamJoist,
                    vec![wall_node],
                );
                socket_joists.push((*socket, joist));
            }
        }
        let mut floor_solids = vec![
            projected_solid(
                geometry,
                owner,
                Vec3::new(
                    midpoint.x + outward.x * (0.12 + (inner_walk - 0.14) * 0.5),
                    run.base_height_metres - 0.07,
                    midpoint.y + outward.y * (0.12 + (inner_walk - 0.14) * 0.5),
                ),
                // Keep the pitched floor skin positively clear of the first
                // downward-defense throat. Rotating a slab whose nominal edge
                // merely touches the throat would otherwise push its lower
                // corner a few millimetres into the opening.
                Vec3::new(length, 0.14, inner_walk - 0.14),
                yaw,
                SolidRole::GalleryFloor,
                vec![floor_node],
            ),
            projected_solid(
                geometry,
                owner,
                Vec3::new(
                    midpoint.x + outward.x * (projection - 0.07),
                    run.base_height_metres - 0.07,
                    midpoint.y + outward.y * (projection - 0.07),
                ),
                Vec3::new(length, 0.14, 0.14),
                yaw,
                SolidRole::GalleryFloor,
                vec![floor_node],
            ),
        ];
        let outer_breastwork_bearing = floor_solids.pop().expect("outer gallery bearing");
        geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == outer_breastwork_bearing)
            .expect("outer gallery bearing solid")
            .role = SolidRole::ProjectionSupport;
        let local_positive_z = Vec2::new(yaw.sin(), yaw.cos());
        let floor_crossfall = 0.025 * (-outward).dot(local_positive_z).signum();
        let floor = geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == floor_solids[0])
            .expect("new projected gallery floor");
        floor.crossfall_radians = floor_crossfall;
        floor.longfall_radians = 0.003;
        let channel_length = length - 0.11;
        let channel_centre = midpoint - tangent * 0.055 + outward * 0.06;
        let drainage_floor = projected_solid(
            geometry,
            owner,
            Vec3::new(
                channel_centre.x,
                run.base_height_metres - 0.055,
                channel_centre.y,
            ),
            Vec3::new(channel_length, 0.06, 0.12),
            yaw,
            SolidRole::DrainageFloor,
            vec![floor_node],
        );
        geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == drainage_floor)
            .expect("new projected channel floor")
            .longfall_radians = -0.018;
        let bay_length = length / bay_count as f32;
        let mut throat_voids = Vec::new();
        for index in 0..bay_count {
            let along = -length * 0.5 + (index as f32 + 0.5) * bay_length;
            let throat_centre =
                midpoint + tangent * along + outward * (inner_walk + throat_depth * 0.5);
            let throat = projected_void(
                geometry,
                owner,
                ResolvedBounds {
                    min: Vec3::new(
                        throat_centre.x
                            - tangent.x.abs() * (bay_length - 0.18) * 0.5
                            - outward.x.abs() * throat_depth * 0.5,
                        run.base_height_metres - 0.17,
                        throat_centre.y
                            - tangent.y.abs() * (bay_length - 0.18) * 0.5
                            - outward.y.abs() * throat_depth * 0.5,
                    ),
                    max: Vec3::new(
                        throat_centre.x
                            + tangent.x.abs() * (bay_length - 0.18) * 0.5
                            + outward.x.abs() * throat_depth * 0.5,
                        run.base_height_metres + 0.03,
                        throat_centre.y
                            + tangent.y.abs() * (bay_length - 0.18) * 0.5
                            + outward.y.abs() * throat_depth * 0.5,
                    ),
                },
                VoidRole::DefenseThroat,
            );
            throat_voids.push(throat);
            let stance = Vec3::new(
                throat_centre.x - outward.x * 0.42,
                run.base_height_metres,
                throat_centre.y - outward.y * 0.42,
            );
            let origin = Vec3::new(
                throat_centre.x + outward.x * throat_depth * 0.48,
                run.base_height_metres + 0.025,
                throat_centre.y + outward.y * throat_depth * 0.48,
            );
            geometry
                .projected_defense_working_points
                .push(ProjectedDefenseWorkingPoint {
                    owner,
                    aperture: throat,
                    stance,
                    eye: stance + Vec3::Y * 1.55,
                    support_solid: floor_solids[0],
                });
            for (range, distance) in [
                (ProjectedDefenseRange::Near, 0.75_f32),
                (ProjectedDefenseRange::Middle, 1.6_f32),
                (ProjectedDefenseRange::Far, 3.0_f32),
            ] {
                geometry.projected_defense_rays.push(ProjectedDefenseRay {
                    owner,
                    throat,
                    stance,
                    origin,
                    target: Vec3::new(
                        throat_centre.x + outward.x * distance,
                        0.25,
                        throat_centre.y + outward.y * distance,
                    ),
                    range,
                });
            }
        }
        let outer_wall_centre = midpoint + outward * (projection + 0.09);
        let wall_role = if material == ProjectedDefenseMaterial::Timber {
            SolidRole::FrameMember
        } else {
            SolidRole::DefenseWall
        };
        let aperture_half_width = 0.12;
        let aperture_along = bay_length.min(length * 0.28);
        let middle_intervals = [
            (-length * 0.5, -aperture_along - aperture_half_width),
            (
                -aperture_along + aperture_half_width,
                aperture_along - aperture_half_width,
            ),
            (aperture_along + aperture_half_width, length * 0.5),
        ];
        let wall_segments = [(0.0, length, 0.55, 0.275), (0.0, length, 0.14, 1.09)]
            .into_iter()
            .chain(
                middle_intervals
                    .into_iter()
                    .map(|(start, end)| ((start + end) * 0.5, end - start, 0.47, 0.785)),
            );
        let mut enclosure_wall_solids = Vec::new();
        for (along, segment_length, height, vertical_centre) in wall_segments {
            if segment_length <= 0.05 {
                continue;
            }
            let centre = outer_wall_centre + tangent * along;
            enclosure_wall_solids.push(projected_solid(
                geometry,
                owner,
                Vec3::new(centre.x, run.base_height_metres + vertical_centre, centre.y),
                Vec3::new(segment_length, height, 0.18),
                yaw,
                wall_role,
                vec![floor_node],
            ));
        }
        if material == ProjectedDefenseMaterial::Timber {
            for index in 0..=bay_count {
                let anchor = run.start.lerp(run.end, index as f32 / bay_count as f32)
                    + outward * (projection + 0.09);
                projected_solid(
                    geometry,
                    owner,
                    Vec3::new(anchor.x, run.base_height_metres + 1.1, anchor.y),
                    Vec3::new(0.14, 2.2, 0.14),
                    yaw,
                    SolidRole::FrameMember,
                    vec![floor_node],
                );
            }
        }
        let access_portal = host.portal.expect("operational defense host portal");
        let landing_centre = midpoint - outward * 0.17;
        let access_landing = projected_solid(
            geometry,
            owner,
            Vec3::new(
                landing_centre.x,
                run.base_height_metres - 0.07,
                landing_centre.y,
            ),
            Vec3::new(0.86, 0.14, 0.66),
            yaw,
            SolidRole::Landing,
            vec![floor_node],
        );
        let mut firing_apertures = Vec::new();
        for side in [-1.0_f32, 1.0] {
            let aperture = outer_wall_centre + tangent * aperture_along * side;
            let aperture_id = projected_void(
                geometry,
                owner,
                ResolvedBounds {
                    min: Vec3::new(
                        aperture.x - 0.09,
                        run.base_height_metres + 0.55,
                        aperture.y - 0.09,
                    ),
                    max: Vec3::new(
                        aperture.x + 0.09,
                        run.base_height_metres + 1.02,
                        aperture.y + 0.09,
                    ),
                },
                VoidRole::FiringAperture,
            );
            firing_apertures.push(aperture_id);
            let stance = Vec3::new(
                aperture.x - outward.x * 0.52,
                run.base_height_metres,
                aperture.y - outward.y * 0.52,
            );
            let eye = Vec3::new(aperture.x, run.base_height_metres + 0.79, aperture.y);
            geometry
                .projected_defense_working_points
                .push(ProjectedDefenseWorkingPoint {
                    owner,
                    aperture: aperture_id,
                    stance,
                    eye,
                    support_solid: floor_solids[0],
                });
            for (range, distance) in [
                (ProjectedDefenseRange::Near, 2.0_f32),
                (ProjectedDefenseRange::Middle, 6.0_f32),
                (ProjectedDefenseRange::Far, 12.0_f32),
            ] {
                geometry.projected_defense_rays.push(ProjectedDefenseRay {
                    owner,
                    throat: aperture_id,
                    stance,
                    origin: eye,
                    target: eye + Vec3::new(outward.x * distance, -0.55, outward.y * distance),
                    range,
                });
            }
        }
        let mut weather_catchments = Vec::new();
        let mut weathering_solids = Vec::new();
        let mut roof_support_solids = Vec::new();
        let mut roof_bearing_node = None;
        if roofed {
            let roof_depth = projection + 0.45;
            let roof_support = if kind == ProjectedDefenseKind::Breteche {
                let inner_bearing = StructuralNodeId(floor_node.0 + 1);
                let outer_bearing = StructuralNodeId(floor_node.0 + 2);
                let roof_bearing = StructuralNodeId(floor_node.0 + 3);
                for (id, position) in [
                    (inner_bearing, midpoint),
                    (outer_bearing, outer_wall_centre),
                ] {
                    geometry.structural_nodes.push(StructuralNode {
                        id,
                        owner,
                        kind: StructuralNodeKind::GalleryFrame,
                        position: Vec3::new(position.x, run.base_height_metres, position.y),
                        supported_by: vec![floor_node],
                        grounded: false,
                    });
                }
                geometry.structural_nodes.push(StructuralNode {
                    id: roof_bearing,
                    owner,
                    kind: StructuralNodeKind::GalleryFrame,
                    position: Vec3::new(
                        midpoint.x + outward.x * projection * 0.55,
                        run.base_height_metres + 2.18,
                        midpoint.y + outward.y * projection * 0.55,
                    ),
                    supported_by: vec![inner_bearing, outer_bearing],
                    grounded: false,
                });
                roof_bearing_node = Some(roof_bearing);
                roof_bearing
            } else {
                floor_node
            };
            let roof_id = projected_solid(
                geometry,
                owner,
                Vec3::new(
                    midpoint.x + outward.x * projection * 0.55,
                    run.base_height_metres + 2.25,
                    midpoint.y + outward.y * projection * 0.55,
                ),
                Vec3::new(length + 0.35, 0.14, roof_depth),
                yaw,
                SolidRole::DefenseRoof,
                vec![roof_support],
            );
            let (catchment, solids) = resolve_linear_roof_weathering(
                geometry,
                owner,
                roof_id,
                midpoint + outward * projection * 0.55,
                tangent,
                outward,
                length + 0.35,
                roof_depth,
                yaw,
                roof_support,
            );
            weather_catchments.push(catchment);
            weathering_solids.extend(solids);
            if kind == ProjectedDefenseKind::Breteche {
                let roof = geometry
                    .solids
                    .iter()
                    .find(|solid| solid.id == roof_id)
                    .expect("resolved bretèche roof")
                    .clone();
                let roof_midpoint = Vec2::new(roof.centre.x, roof.centre.z);
                let underside_at = |point: Vec2| {
                    let offset = (point - roof_midpoint).dot(outward);
                    roof.centre.y - offset * roof.crossfall_radians.abs().tan() - roof.size.y * 0.5
                };
                let inner_plate_plan = midpoint + outward * 0.02;
                let outer_plate_plan = outer_wall_centre;
                let inner_underside = underside_at(inner_plate_plan);
                let outer_underside = underside_at(outer_plate_plan);
                let plate_height = 0.16;
                let inner_bearing = StructuralNodeId(floor_node.0 + 1);
                let outer_bearing = StructuralNodeId(floor_node.0 + 2);

                // Extend the already-resolved upper outer-wall band to the low
                // wall plate. This retains the two firing-loop cuts below it
                // while removing the formerly open metre-high sky band.
                let upper_wall = enclosure_wall_solids
                    .get(1)
                    .copied()
                    .expect("bretèche upper enclosure wall");
                let upper_wall_bottom = run.base_height_metres + 1.02;
                let upper_wall_top = outer_underside - plate_height;
                let wall = geometry
                    .solids
                    .iter_mut()
                    .find(|solid| solid.id == upper_wall)
                    .expect("bretèche upper enclosure wall solid");
                wall.centre.y = (upper_wall_bottom + upper_wall_top) * 0.5;
                wall.size.y = upper_wall_top - upper_wall_bottom;

                for side in [-1.0_f32, 1.0] {
                    let post_plan = inner_plate_plan + tangent * side * (length * 0.5 - 0.38);
                    let post_height = inner_underside - plate_height - run.base_height_metres;
                    roof_support_solids.push(projected_solid(
                        geometry,
                        owner,
                        Vec3::new(
                            post_plan.x,
                            run.base_height_metres + post_height * 0.5,
                            post_plan.y,
                        ),
                        Vec3::new(0.18, post_height, 0.18),
                        yaw,
                        SolidRole::FrameMember,
                        vec![floor_node],
                    ));
                }
                for (plan, underside, bearing) in [
                    (inner_plate_plan, inner_underside, inner_bearing),
                    (outer_plate_plan, outer_underside, outer_bearing),
                ] {
                    let plate = projected_solid(
                        geometry,
                        owner,
                        Vec3::new(plan.x, underside - plate_height * 0.5, plan.y),
                        Vec3::new(length + 0.18, plate_height, 0.18),
                        yaw,
                        SolidRole::RoofPlate,
                        vec![bearing],
                    );
                    geometry
                        .solids
                        .iter_mut()
                        .find(|solid| solid.id == plate)
                        .expect("bretèche roof plate")
                        .crossfall_radians = roof.crossfall_radians;
                    roof_support_solids.push(plate);
                }
                roof_support_solids.push(upper_wall);
            }
        } else if material == ProjectedDefenseMaterial::Masonry {
            let (catchment, solids) = resolve_linear_coping_weathering(
                geometry,
                owner,
                outer_wall_centre,
                tangent,
                outward,
                length,
                run.base_height_metres + 1.22,
                yaw,
                floor_node,
            );
            weather_catchments.push(catchment);
            weathering_solids.extend(solids);
        }
        let drainage_surface = projected_surface(
            geometry,
            owner,
            ResolvedBounds {
                min: Vec3::new(
                    midpoint.x - tangent.x.abs() * length * 0.5,
                    run.base_height_metres,
                    midpoint.y - tangent.y.abs() * length * 0.5,
                ),
                max: Vec3::new(
                    midpoint.x + tangent.x.abs() * length * 0.5 + outward.x.abs() * projection,
                    run.base_height_metres + 0.02,
                    midpoint.y + tangent.y.abs() * length * 0.5 + outward.y.abs() * projection,
                ),
            },
            SurfaceRole::Drainage,
        );
        let drain_inlet = Vec3::new(
            midpoint.x + tangent.x * (length * 0.5 - 0.11) + outward.x * 0.06,
            run.base_height_metres - 0.03,
            midpoint.y + tangent.y * (length * 0.5 - 0.11) + outward.y * 0.06,
        );
        let drain_route = projected_edge_drain(geometry, owner, drain_inlet, tangent);
        let catchment_id =
            ResolvedItemId((7_u64 << 60) | geometry.drainage_catchments.len() as u64);
        geometry.drainage_catchments.push(DrainageCatchment {
            id: catchment_id,
            owner,
            walk_solid: floor_solids[0],
            toe_channel_solids: vec![drainage_floor],
            drainage_surface,
            outlet_route: drain_route,
            centre: Vec3::new(midpoint.x, run.base_height_metres, midpoint.y),
            tangent,
            outward: -outward,
            length_metres: length,
            width_metres: inner_walk - 0.12,
            inner_elevation_metres: run.base_height_metres,
            outer_elevation_metres: run.base_height_metres - 0.025,
            outlet_along_metres: length * 0.5 - 0.11,
        });
        assemblies.push(ProjectedDefenseAssembly {
            owner,
            host_owner: host.owner,
            host_wall_solids: host.walls,
            host_buttress_solids: host.buttresses,
            host_source_walls: host.sources,
            host_top_elevation_metres: host.top_elevation_metres,
            host_topology: host.topology,
            host_walk_solid: host.walk,
            host_portal_void: Some(access_portal),
            host_bond: Some(bond_id),
            beam_socket_voids: host.sockets,
            socket_joists,
            kind,
            material,
            phase,
            deployment,
            tactical_target,
            path: ProjectedDefensePath::Linear {
                start: run.start,
                end: run.end,
                outward: run.outward,
            },
            floor_elevation_metres: run.base_height_metres,
            clear_width_metres: inner_walk,
            clear_height_metres: 2.05,
            projection_metres: projection,
            breastwork_height_metres: 1.16,
            roofed,
            floor_solids,
            throat_voids,
            access_portal: Some(access_portal),
            access_landing: Some(access_landing),
            firing_apertures,
            support_nodes,
            drain_route: Some(drain_route),
            drainage_catchments: vec![catchment_id],
            weather_catchments,
            weathering_solids,
            roof_support_solids,
            roof_bearing_node,
        });
    }
    let dimensions = Vec2::new(
        f32::from(program.footprint.dimensions().0) * CELL_SIZE_METRES,
        f32::from(program.footprint.dimensions().1) * CELL_SIZE_METRES,
    );
    let plan_centre = dimensions * 0.5;
    for (index, bartizan) in bartizans.iter().copied().enumerate() {
        let owner = GeometryOwnerId(2_000 + index as u32);
        let delta = bartizan.centre - plan_centre;
        let outward_direction = if delta.x.abs() >= delta.y.abs() {
            if delta.x >= 0.0 {
                Direction::East
            } else {
                Direction::West
            }
        } else if delta.y >= 0.0 {
            Direction::North
        } else {
            Direction::South
        };
        let outward = direction_vector(outward_direction);
        let tangent = Vec2::new(-outward.y, outward.x);
        let yaw = -tangent.y.atan2(tangent.x);
        let host_midpoint = match outward_direction {
            Direction::East => Vec2::new(dimensions.x, bartizan.centre.y),
            Direction::West => Vec2::new(0.0, bartizan.centre.y),
            Direction::North => Vec2::new(bartizan.centre.x, dimensions.y),
            Direction::South => Vec2::new(bartizan.centre.x, 0.0),
        };
        let mut host = resolve_linear_defense_host(
            geometry,
            storeys,
            100 + index,
            BattlementRun {
                start: host_midpoint - tangent * bartizan.radius_metres,
                end: host_midpoint + tangent * bartizan.radius_metres,
                base_height_metres: bartizan.base_height_metres,
                kind: BattlementKind::Breteche,
                outward: outward_direction,
            },
            None,
            true,
        );
        let buttress_depth = bartizan.radius_metres * 0.92;
        let buttress_centre = host_midpoint + outward * buttress_depth * 0.5;
        let buttress_top = bartizan.base_height_metres - 0.14;
        let buttress = projected_solid(
            geometry,
            host.owner,
            Vec3::new(buttress_centre.x, buttress_top * 0.5, buttress_centre.y),
            Vec3::new(0.18, buttress_top, buttress_depth),
            yaw,
            SolidRole::DefenseHostButtress,
            vec![host.bearing],
        );
        host.buttresses.push(buttress);
        host.topology = ProjectedDefenseHostTopology::Buttress;
        let wall_node = host.bearing;
        let floor_node = StructuralNodeId(wall_node.0 + 10);
        let host_bond = ResolvedItemId((6_u64 << 60) | (10_000 + index) as u64);
        geometry.junction_bonds.push(JunctionBond {
            id: host_bond,
            owners: [host.owner, owner],
            bounds: ResolvedBounds {
                min: Vec3::new(
                    host_midpoint.x
                        - tangent.x.abs() * bartizan.radius_metres
                        - outward.x.abs() * 0.75,
                    bartizan.base_height_metres - 0.6,
                    host_midpoint.y
                        - tangent.y.abs() * bartizan.radius_metres
                        - outward.y.abs() * 0.75,
                ),
                max: Vec3::new(
                    host_midpoint.x
                        + tangent.x.abs() * bartizan.radius_metres
                        + outward.x.abs() * 0.75,
                    bartizan.base_height_metres + bartizan.height_metres + 0.3,
                    host_midpoint.y
                        + tangent.y.abs() * bartizan.radius_metres
                        + outward.y.abs() * 0.75,
                ),
            },
            minimum_interface_area_square_metres: 0.08,
            maximum_penetration_metres: 0.18,
        });
        let mut corbel_nodes = Vec::new();
        for (index, offset) in [-0.5_f32, 0.0, 0.5].into_iter().enumerate() {
            let corbel_node = StructuralNodeId(wall_node.0 + 1 + index as u64);
            corbel_nodes.push(corbel_node);
            let centre =
                bartizan.centre - outward * bartizan.radius_metres * 0.35 + tangent * offset;
            geometry.structural_nodes.push(StructuralNode {
                id: corbel_node,
                owner,
                kind: StructuralNodeKind::ProjectionCorbel,
                position: Vec3::new(centre.x, bartizan.base_height_metres - 0.35, centre.y),
                supported_by: vec![wall_node],
                grounded: false,
            });
            projected_solid(
                geometry,
                owner,
                Vec3::new(centre.x, bartizan.base_height_metres - 0.35, centre.y),
                Vec3::new(0.22, 0.48, bartizan.radius_metres * 1.2),
                yaw,
                SolidRole::ProjectionSupport,
                vec![wall_node],
            );
        }
        geometry.structural_nodes.push(StructuralNode {
            id: floor_node,
            owner,
            kind: StructuralNodeKind::GalleryFrame,
            position: Vec3::new(
                bartizan.centre.x,
                bartizan.base_height_metres,
                bartizan.centre.y,
            ),
            supported_by: corbel_nodes.clone(),
            grounded: false,
        });
        for node in &corbel_nodes {
            let position = geometry
                .structural_nodes
                .iter()
                .find(|candidate| candidate.id == *node)
                .expect("bartizan support node")
                .position;
            let tangent_extent = tangent.abs() * 0.11;
            let outward_extent = outward.abs() * (bartizan.radius_metres * 0.55);
            let extent = tangent_extent + outward_extent;
            geometry.support_interfaces.push(SupportInterface {
                id: ResolvedItemId((4_u64 << 60) | geometry.support_interfaces.len() as u64),
                owner,
                node: *node,
                bounds: ResolvedBounds {
                    min: Vec3::new(
                        position.x - extent.x,
                        bartizan.base_height_metres - 0.09,
                        position.z - extent.y,
                    ),
                    max: Vec3::new(
                        position.x + extent.x,
                        bartizan.base_height_metres - 0.06,
                        position.z + extent.y,
                    ),
                },
            });
        }
        let segments = 16;
        let half_span = bartizan.radius_metres * 0.82;
        // The resolved throat is an axis-aligned subtraction while the
        // bartizan floor bays are wall-local cuboids. Keep the authoritative
        // opening at 0.36 m, but trim the surrounding local bays by its
        // projected diagonal extent plus a construction joint.
        let throat_void_half = 0.18;
        let throat_clear_half = throat_void_half * (outward.x.abs() + outward.y.abs()) + 0.03;
        let throat_inner = bartizan.radius_metres * 0.55 - throat_clear_half;
        let inner_edge = -bartizan.radius_metres * 0.82;
        let outer_edge = bartizan.radius_metres * 0.82;
        let mut floor_solids = vec![projected_solid(
            geometry,
            owner,
            Vec3::new(
                bartizan.centre.x + outward.x * (inner_edge + throat_inner) * 0.5,
                bartizan.base_height_metres - 0.07,
                bartizan.centre.y + outward.y * (inner_edge + throat_inner) * 0.5,
            ),
            Vec3::new(half_span * 2.0, 0.14, throat_inner - inner_edge),
            yaw,
            SolidRole::GalleryFloor,
            vec![floor_node],
        )];
        let side_width = half_span - throat_clear_half;
        for side in [-1.0_f32, 1.0] {
            let centre = bartizan.centre
                + outward * ((throat_inner + outer_edge) * 0.5)
                + tangent * side * ((throat_clear_half + half_span) * 0.5);
            floor_solids.push(projected_solid(
                geometry,
                owner,
                Vec3::new(centre.x, bartizan.base_height_metres - 0.07, centre.y),
                Vec3::new(side_width, 0.14, outer_edge - throat_inner),
                yaw,
                SolidRole::GalleryFloor,
                vec![floor_node],
            ));
        }
        let throat_centre = bartizan.centre + outward * bartizan.radius_metres * 0.55;
        let throat = projected_void(
            geometry,
            owner,
            ResolvedBounds {
                min: Vec3::new(
                    throat_centre.x - throat_void_half,
                    bartizan.base_height_metres - 0.18,
                    throat_centre.y - throat_void_half,
                ),
                max: Vec3::new(
                    throat_centre.x + throat_void_half,
                    bartizan.base_height_metres + 0.03,
                    throat_centre.y + throat_void_half,
                ),
            },
            VoidRole::DefenseThroat,
        );
        let bartizan_stance = Vec3::new(
            throat_centre.x - outward.x * 0.42,
            bartizan.base_height_metres,
            throat_centre.y - outward.y * 0.42,
        );
        let bartizan_origin = Vec3::new(
            throat_centre.x,
            bartizan.base_height_metres + 0.025,
            throat_centre.y,
        );
        geometry
            .projected_defense_working_points
            .push(ProjectedDefenseWorkingPoint {
                owner,
                aperture: throat,
                stance: bartizan_stance,
                eye: bartizan_stance + Vec3::Y * 1.55,
                support_solid: floor_solids[0],
            });
        for (range, distance) in [
            (ProjectedDefenseRange::Near, 0.75_f32),
            (ProjectedDefenseRange::Middle, 1.4_f32),
            (ProjectedDefenseRange::Far, 2.6_f32),
        ] {
            geometry.projected_defense_rays.push(ProjectedDefenseRay {
                owner,
                throat,
                stance: bartizan_stance,
                origin: bartizan_origin,
                target: Vec3::new(
                    throat_centre.x + outward.x * distance,
                    0.25,
                    throat_centre.y + outward.y * distance,
                ),
                range,
            });
        }
        let inward = -outward;
        let access_portal = host.portal.expect("bartizan host access portal");
        let landing = host_midpoint - outward * 0.08;
        let access_landing = projected_solid(
            geometry,
            owner,
            Vec3::new(landing.x, bartizan.base_height_metres - 0.07, landing.y),
            Vec3::new(0.86, 0.14, 0.66),
            yaw,
            SolidRole::Landing,
            vec![floor_node],
        );
        let mut firing_apertures = Vec::new();
        for side in [-2_i32, 0, 2] {
            let side = side as f32 * std::f32::consts::TAU / segments as f32;
            let direction = Vec2::new(
                (outward.y.atan2(outward.x) + side).cos(),
                (outward.y.atan2(outward.x) + side).sin(),
            );
            let aperture_half = Vec2::splat(0.065);
            let wall_centre = bartizan.centre + direction * bartizan.radius_metres;
            let aperture = projected_void(
                geometry,
                owner,
                ResolvedBounds {
                    min: Vec3::new(
                        wall_centre.x - aperture_half.x,
                        bartizan.base_height_metres + 0.75,
                        wall_centre.y - aperture_half.y,
                    ),
                    max: Vec3::new(
                        wall_centre.x + aperture_half.x,
                        bartizan.base_height_metres + 1.22,
                        wall_centre.y + aperture_half.y,
                    ),
                },
                VoidRole::FiringAperture,
            );
            firing_apertures.push(aperture);
            let stance_plan = if side.abs() < 0.01 {
                bartizan.centre + outward * 0.20
            } else {
                bartizan.centre + direction * (bartizan.radius_metres - 0.38)
            };
            let stance = Vec3::new(stance_plan.x, bartizan.base_height_metres, stance_plan.y);
            let eye = Vec3::new(
                wall_centre.x,
                bartizan.base_height_metres + 0.985,
                wall_centre.y,
            );
            let support_solid = floor_solids
                .iter()
                .copied()
                .find(|id| {
                    geometry
                        .solids
                        .iter()
                        .find(|solid| solid.id == *id)
                        .is_some_and(|solid| {
                            resolved_solid_contains_point(solid, stance - Vec3::Y * 0.02, 0.08)
                        })
                })
                .unwrap_or(floor_solids[0]);
            geometry
                .projected_defense_working_points
                .push(ProjectedDefenseWorkingPoint {
                    owner,
                    aperture,
                    stance,
                    eye,
                    support_solid,
                });
            for (range, distance) in [
                (ProjectedDefenseRange::Near, 2.0_f32),
                (ProjectedDefenseRange::Middle, 6.0_f32),
                (ProjectedDefenseRange::Far, 12.0_f32),
            ] {
                geometry.projected_defense_rays.push(ProjectedDefenseRay {
                    owner,
                    throat: aperture,
                    stance,
                    origin: eye,
                    target: eye + Vec3::new(direction.x * distance, -0.55, direction.y * distance),
                    range,
                });
            }
        }
        for segment in 0..segments {
            let angle = segment as f32 * std::f32::consts::TAU / segments as f32;
            let radial = Vec2::new(angle.cos(), angle.sin());
            // Three inward facets form the real doorway chord; unlike the old
            // half-cylinder deletion, every other facet remains structural.
            if radial.dot(inward) > 0.88 {
                continue;
            }
            let centre = bartizan.centre + radial * bartizan.radius_metres;
            let facet_length =
                2.0 * bartizan.radius_metres * (std::f32::consts::PI / segments as f32).tan()
                    + 0.03;
            let aperture = firing_apertures
                .iter()
                .filter_map(|id| {
                    geometry
                        .voids
                        .iter()
                        .find(|void| void.id == *id)
                        .and_then(|void| {
                            let aperture_centre = (void.bounds.min + void.bounds.max) * 0.5;
                            let distance =
                                Vec2::new(aperture_centre.x, aperture_centre.z).distance(centre);
                            (distance < 0.55).then_some((distance, *id, *void))
                        })
                })
                .min_by(|left, right| left.0.total_cmp(&right.0))
                .map(|(_, id, void)| (id, void));
            let shell_yaw = -angle - std::f32::consts::FRAC_PI_2;
            if let Some((_id, aperture)) = aperture {
                let lower_height = aperture.bounds.min.y - bartizan.base_height_metres;
                let upper_height =
                    bartizan.base_height_metres + bartizan.height_metres - aperture.bounds.max.y;
                for (height, vertical_centre) in [
                    (
                        lower_height,
                        bartizan.base_height_metres + lower_height * 0.5,
                    ),
                    (upper_height, aperture.bounds.max.y + upper_height * 0.5),
                ] {
                    if height > 0.02 {
                        projected_solid(
                            geometry,
                            owner,
                            Vec3::new(centre.x, vertical_centre, centre.y),
                            Vec3::new(facet_length, height, 0.18),
                            shell_yaw,
                            SolidRole::BartizanShell,
                            vec![floor_node],
                        );
                    }
                }
                let facet_tangent = Vec2::new(-radial.y, radial.x);
                let aperture_centre = (aperture.bounds.min + aperture.bounds.max) * 0.5;
                let aperture_half_bounds = (aperture.bounds.max - aperture.bounds.min) * 0.5;
                let aperture_offset =
                    (Vec2::new(aperture_centre.x, aperture_centre.z) - centre).dot(facet_tangent);
                let splayed_half_width = facet_tangent.x.abs() * aperture_half_bounds.x
                    + facet_tangent.y.abs() * aperture_half_bounds.z
                    + 0.06;
                let opening_min = (aperture_offset - splayed_half_width).max(-facet_length * 0.5);
                let opening_max = (aperture_offset + splayed_half_width).min(facet_length * 0.5);
                for (side_width, side_offset) in [
                    (
                        opening_min + facet_length * 0.5,
                        (-facet_length * 0.5 + opening_min) * 0.5,
                    ),
                    (
                        facet_length * 0.5 - opening_max,
                        (opening_max + facet_length * 0.5) * 0.5,
                    ),
                ] {
                    if side_width <= 0.01 {
                        continue;
                    }
                    let side_centre = centre + facet_tangent * side_offset;
                    projected_solid(
                        geometry,
                        owner,
                        Vec3::new(
                            side_centre.x,
                            (aperture.bounds.min.y + aperture.bounds.max.y) * 0.5,
                            side_centre.y,
                        ),
                        Vec3::new(
                            side_width,
                            aperture.bounds.max.y - aperture.bounds.min.y,
                            0.18,
                        ),
                        shell_yaw,
                        SolidRole::BartizanShell,
                        vec![floor_node],
                    );
                }
            } else {
                projected_solid(
                    geometry,
                    owner,
                    Vec3::new(
                        centre.x,
                        bartizan.base_height_metres + bartizan.height_metres * 0.5,
                        centre.y,
                    ),
                    Vec3::new(facet_length, bartizan.height_metres, 0.18),
                    shell_yaw,
                    SolidRole::BartizanShell,
                    vec![floor_node],
                );
            }
        }
        for floor_id in &floor_solids {
            geometry
                .solids
                .iter_mut()
                .find(|solid| solid.id == *floor_id)
                .expect("bartizan floor")
                .longfall_radians = -0.022;
        }
        let channel_yaw = -outward.y.atan2(outward.x);
        let bartizan_channel_centre =
            bartizan.centre + tangent * (half_span + 0.06) - outward * 0.055;
        let bartizan_channel = projected_solid(
            geometry,
            owner,
            Vec3::new(
                bartizan_channel_centre.x,
                bartizan.base_height_metres - 0.055,
                bartizan_channel_centre.y,
            ),
            Vec3::new(half_span * 2.0 - 0.11, 0.06, 0.12),
            channel_yaw,
            SolidRole::DrainageFloor,
            vec![floor_node],
        );
        geometry
            .solids
            .iter_mut()
            .find(|solid| solid.id == bartizan_channel)
            .expect("bartizan drainage channel")
            .longfall_radians = -0.018;
        let mut weather_catchments = Vec::new();
        let mut weathering_solids = Vec::new();
        if bartizan.roofed {
            let roof_extent = bartizan.radius_metres * 2.25;
            let roof_id = projected_solid(
                geometry,
                owner,
                Vec3::new(
                    bartizan.centre.x,
                    bartizan.base_height_metres + bartizan.height_metres + 0.08,
                    bartizan.centre.y,
                ),
                Vec3::new(roof_extent, 0.16, roof_extent),
                yaw,
                SolidRole::DefenseRoof,
                vec![floor_node],
            );
            let (catchment, solids) = resolve_linear_roof_weathering(
                geometry,
                owner,
                roof_id,
                bartizan.centre,
                tangent,
                outward,
                roof_extent,
                roof_extent,
                yaw,
                floor_node,
            );
            weather_catchments.push(catchment);
            weathering_solids.extend(solids);
        } else {
            for segment in 0..segments {
                let angle = segment as f32 * std::f32::consts::TAU / segments as f32;
                let radial = Vec2::new(angle.cos(), angle.sin());
                if radial.dot(inward) > 0.88 {
                    continue;
                }
                let facet_tangent = Vec2::new(-radial.y, radial.x);
                let facet_length =
                    2.0 * bartizan.radius_metres * (std::f32::consts::PI / segments as f32).tan()
                        + 0.03;
                let facet_centre = bartizan.centre + radial * bartizan.radius_metres;
                let facet_yaw = -angle - std::f32::consts::FRAC_PI_2;
                let (catchment, solids) = resolve_linear_coping_weathering(
                    geometry,
                    owner,
                    facet_centre,
                    facet_tangent,
                    radial,
                    facet_length,
                    bartizan.base_height_metres + bartizan.height_metres + 0.08,
                    facet_yaw,
                    floor_node,
                );
                weather_catchments.push(catchment);
                weathering_solids.extend(solids);
            }
        }
        let drainage_surface = projected_surface(
            geometry,
            owner,
            ResolvedBounds {
                min: Vec3::new(
                    bartizan.centre.x - bartizan.radius_metres,
                    bartizan.base_height_metres,
                    bartizan.centre.y - bartizan.radius_metres,
                ),
                max: Vec3::new(
                    bartizan.centre.x + bartizan.radius_metres,
                    bartizan.base_height_metres + 0.02,
                    bartizan.centre.y + bartizan.radius_metres,
                ),
            },
            SurfaceRole::Drainage,
        );
        let drain_route = projected_edge_drain(
            geometry,
            owner,
            Vec3::new(
                bartizan.centre.x + tangent.x * (half_span + 0.06) + outward.x * (half_span - 0.11),
                bartizan.base_height_metres - 0.03,
                bartizan.centre.y + tangent.y * (half_span + 0.06) + outward.y * (half_span - 0.11),
            ),
            outward,
        );
        let catchment_id =
            ResolvedItemId((7_u64 << 60) | geometry.drainage_catchments.len() as u64);
        geometry.drainage_catchments.push(DrainageCatchment {
            id: catchment_id,
            owner,
            walk_solid: floor_solids[0],
            toe_channel_solids: vec![bartizan_channel],
            drainage_surface,
            outlet_route: drain_route,
            centre: Vec3::new(
                bartizan.centre.x,
                bartizan.base_height_metres,
                bartizan.centre.y,
            ),
            tangent: outward,
            outward: tangent,
            length_metres: half_span * 2.0,
            width_metres: half_span * 2.0,
            inner_elevation_metres: bartizan.base_height_metres,
            outer_elevation_metres: bartizan.base_height_metres - 0.035,
            outlet_along_metres: half_span - 0.11,
        });
        assemblies.push(ProjectedDefenseAssembly {
            owner,
            host_owner: host.owner,
            host_wall_solids: host.walls,
            host_buttress_solids: host.buttresses,
            host_source_walls: host.sources,
            host_top_elevation_metres: host.top_elevation_metres,
            host_topology: host.topology,
            host_walk_solid: host.walk,
            host_portal_void: Some(access_portal),
            host_bond: Some(host_bond),
            beam_socket_voids: Vec::new(),
            socket_joists: Vec::new(),
            kind: ProjectedDefenseKind::Bartizan,
            material: ProjectedDefenseMaterial::Masonry,
            phase: ProjectedDefensePhase::PermanentMainWork,
            deployment: ProjectedDefenseDeployment::Permanent,
            tactical_target: ProjectedDefenseTarget::ThreatenedCorner,
            path: ProjectedDefensePath::Round {
                centre: bartizan.centre,
                radius_metres: bartizan.radius_metres,
                outward: outward_direction,
            },
            floor_elevation_metres: bartizan.base_height_metres,
            clear_width_metres: bartizan.radius_metres * 1.2,
            clear_height_metres: bartizan.height_metres,
            projection_metres: bartizan.radius_metres,
            breastwork_height_metres: bartizan.height_metres,
            roofed: bartizan.roofed,
            floor_solids,
            throat_voids: vec![throat],
            access_portal: Some(access_portal),
            access_landing: Some(access_landing),
            firing_apertures,
            support_nodes: corbel_nodes,
            drain_route: Some(drain_route),
            drainage_catchments: vec![catchment_id],
            weather_catchments,
            weathering_solids,
            roof_support_solids: Vec::new(),
            roof_bearing_node: None,
        });
    }
    assemblies
}

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
