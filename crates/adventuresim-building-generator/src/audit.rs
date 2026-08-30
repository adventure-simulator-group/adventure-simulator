use std::collections::{BTreeMap, BTreeSet, VecDeque};

use bevy::math::{Quat, Vec2, Vec3};
use geo::{Area, BooleanOps, Coord, LineString, MultiPolygon, Polygon};
use serde::{Deserialize, Serialize};

use crate::{
    BattlementKind, BuildingArchetype, BuildingPlan, CROWN_DRAIN_CHANNEL_WIDTH_METRES,
    CrownJunctionKind, CrownPath, DefensiveCircuit, DefensiveJunction, DefensiveJunctionKind,
    Direction, GateClosureKind, OpeningKind, ProjectedDefenseDeployment, ProjectedDefenseKind,
    ProjectedDefenseMaterial, ProjectedDefensePath, ProjectedDefensePhase, ProjectedDefenseTarget,
    ResolvedItemId, ResolvedSolid, RoofEdgeKind, RoofPiece, RoomKind, SolidRole, Stair,
    StructuralNodeId, SurfaceRole, TowerPortalKind, VoidRole, WALL_THICKNESS_METRES, WallWalk,
};

include!("audit/core.rs");
include!("audit/vertical_circulation.rs");
include!("audit/artillery.rs");
include!("audit/timber_geometry.rs");
include!("audit/timber.rs");
include!("audit/church.rs");
include!("audit/roofs.rs");
include!("audit/wall_openings.rs");
include!("audit/resolved_geometry_helpers.rs");
include!("audit/resolved_geometry.rs");
include!("audit/crowns.rs");
include!("audit/projected_defenses.rs");
include!("audit/gatehouses.rs");
include!("audit/fortification_geometry.rs");
include!("audit/fortified_profile.rs");
include!("audit/gate_defenses.rs");
include!("audit/guard_chambers.rs");
include!("audit/guard_access.rs");
include!("audit/ballistics.rs");
include!("audit/defensive_circuits.rs");
include!("audit/shared_helpers.rs");
include!("audit/roof_timber_checks.rs");
include!("audit/timber_intersections.rs");
include!("audit/mesh.rs");
include!("audit/tests.rs");
