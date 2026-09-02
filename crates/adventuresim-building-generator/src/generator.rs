#![allow(clippy::too_many_arguments)]

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
    TowerPortalKind, TraversalEnvelope, VerticalConnectionRequirement, VoidRole, WallWalk,
};

include!("generator/core.rs");
include!("generator/orchestration.rs");
include!("generator/timber_geometry.rs");
include!("generator/timber.rs");
include!("generator/roof_editing.rs");
include!("generator/layout.rs");
include!("generator/wall_derivation.rs");
include!("generator/window_closures.rs");
include!("generator/internal_partitions.rs");
include!("generator/wall_assemblies.rs");
include!("generator/church_tower.rs");
include!("generator/church_windows.rs");
include!("generator/church_assembly.rs");
include!("generator/cathedral_clerestory.rs");
include!("generator/roof_child_fronts.rs");
include!("generator/round_towers.rs");
include!("generator/artillery_castle.rs");
include!("generator/artillery_openings.rs");
include!("generator/wall_bonds.rs");
include!("generator/roof_geometry.rs");
include!("generator/roof_drainage.rs");
include!("generator/roof_split_drainage.rs");
include!("generator/roof_outlets.rs");
include!("generator/roof_abutments.rs");
include!("generator/roof_cutting.rs");
include!("generator/roof_edge_binding.rs");
include!("generator/roof_resolution.rs");
include!("generator/roof_finalization.rs");
include!("generator/roof_assemblies.rs");
include!("generator/roof_derivation.rs");
include!("generator/tower_derivation.rs");
include!("generator/defense_derivation.rs");
include!("generator/crown_geometry.rs");
include!("generator/drainage_catchments.rs");
include!("generator/defensive_circuits.rs");
include!("generator/gate_defenses.rs");
include!("generator/defense_geometry.rs");
include!("generator/linear_defenses.rs");
include!("generator/projected_defenses.rs");
include!("generator/disjoint_sets.rs");
