use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CastleConstructionPhase {
    InheritedMedieval,
    ArtilleryRetrofit1544,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtilleryMaterial {
    Fieldstone,
    Brick,
    Earth,
    Timber,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ArtilleryCastleAssemblyId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ArtilleryCurtainId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ArtilleryRondelId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ArtilleryStationId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ArtilleryTargetId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ArtilleryRouteNodeId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtilleryStationLevel {
    LowerCasemate,
    UpperTerreplein,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtilleryTargetKind {
    CurtainFoot,
    DitchCorner,
    GateThreshold,
    Bridge,
    Approach,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ArtilleryFireRay {
    pub target_id: ArtilleryTargetId,
    pub origin: Vec3,
    pub target: Vec3,
    pub target_kind: ArtilleryTargetKind,
    pub range: ProjectedDefenseRange,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtilleryDefenseTarget {
    pub id: ArtilleryTargetId,
    pub kind: ArtilleryTargetKind,
    pub centre: Vec3,
    pub half_extent_metres: Vec2,
    pub required_independent_stations: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtilleryFireStation {
    pub id: ArtilleryStationId,
    pub rondel: ArtilleryRondelId,
    pub level: ArtilleryStationLevel,
    pub facing: Vec2,
    pub opening: OpeningAssemblyId,
    pub stance_surface: ResolvedItemId,
    pub mount_solid: ResolvedItemId,
    pub recoil_envelope: ResolvedBounds,
    pub smoke_vent: Option<ResolvedItemId>,
    pub rays: Vec<ArtilleryFireRay>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtilleryCurtainAssembly {
    pub id: ArtilleryCurtainId,
    pub owner: GeometryOwnerId,
    pub outward: Direction,
    pub inner_start: GridPoint,
    pub inner_end: GridPoint,
    pub total_depth: GridLength,
    pub height_metres: f32,
    pub revetment_solids: Vec<ResolvedItemId>,
    pub earth_solids: Vec<ResolvedItemId>,
    pub retaining_solids: Vec<ResolvedItemId>,
    pub terreplein_solid: ResolvedItemId,
    pub parapet_solid: ResolvedItemId,
    pub route_surface: ResolvedItemId,
    pub drainage_catchment: ResolvedItemId,
    pub drainage_route: ResolvedItemId,
    pub suppressed_source_walls: Vec<WallSourceId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtilleryRondelAssembly {
    pub id: ArtilleryRondelId,
    pub owner: GeometryOwnerId,
    pub anchor: GridPoint,
    pub diameter: CellDiameter,
    pub shell: GridLength,
    pub adjoining_curtains: [ArtilleryCurtainId; 2],
    pub curtain_bonds: [ResolvedItemId; 2],
    pub shell_solid: ResolvedItemId,
    pub earth_solids: Vec<ResolvedItemId>,
    pub casemate_void: ResolvedItemId,
    pub casemate_floor: ResolvedItemId,
    pub casemate_roof: ResolvedItemId,
    pub terreplein_solid: ResolvedItemId,
    pub parapet_solids: Vec<ResolvedItemId>,
    /// Inner terreplein fall protection around the spiral well. Segments are
    /// omitted only at the authoritative tread-arrival sweep.
    pub stair_guard_solids: Vec<ResolvedItemId>,
    pub route_surfaces: Vec<ResolvedItemId>,
    pub stair_solids: Vec<ResolvedItemId>,
    pub drainage_routes: Vec<ResolvedItemId>,
    pub station_ids: Vec<ArtilleryStationId>,
    pub support_nodes: Vec<StructuralNodeId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeState {
    Deployed,
    Denied,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtilleryBridgeAssembly {
    pub state: BridgeState,
    pub clear_width_metres: f32,
    pub inner_abutment: ResolvedItemId,
    pub outer_abutment: ResolvedItemId,
    pub fixed_solids: Vec<ResolvedItemId>,
    pub removable_solids: Vec<ResolvedItemId>,
    pub denied_gap_void: Option<ResolvedItemId>,
    pub route_surface: Option<ResolvedItemId>,
    pub control_surfaces: [ResolvedItemId; 2],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtilleryDitchAssembly {
    pub width_metres: f32,
    pub depth_metres: f32,
    pub void_id: ResolvedItemId,
    pub scarp_solids: Vec<ResolvedItemId>,
    pub counterscarp_solids: Vec<ResolvedItemId>,
    pub floor_solids: Vec<ResolvedItemId>,
    pub drainage_routes: Vec<ResolvedItemId>,
    pub outlet_surface: ResolvedItemId,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ArtilleryRouteNode {
    pub id: ArtilleryRouteNodeId,
    pub surface: ResolvedItemId,
    pub position: Vec3,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtilleryRouteEdge {
    pub from: ArtilleryRouteNodeId,
    pub to: ArtilleryRouteNodeId,
    pub width_metres: f32,
    pub headroom_metres: f32,
    pub portal_void: Option<ResolvedItemId>,
    pub traversal_surface: Option<ResolvedItemId>,
    pub connector_solids: Vec<ResolvedItemId>,
    /// Ordered floor-centre samples used by the physical occupant sweep.
    pub sweep_path: Vec<Vec3>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtilleryCastleAssembly {
    pub id: ArtilleryCastleAssemblyId,
    pub phase: CastleConstructionPhase,
    pub trace: [GridPoint; 4],
    pub clear_court_size_metres: Vec2,
    pub crown_elevation_metres: f32,
    pub curtains: Vec<ArtilleryCurtainAssembly>,
    pub rondels: Vec<ArtilleryRondelAssembly>,
    pub stations: Vec<ArtilleryFireStation>,
    pub defense_targets: Vec<ArtilleryDefenseTarget>,
    pub ditch: ArtilleryDitchAssembly,
    pub bridge: ArtilleryBridgeAssembly,
    pub gate_passage_void: ResolvedItemId,
    pub gate_closure_solids: Vec<ResolvedItemId>,
    pub gate_chamber_solids: Vec<ResolvedItemId>,
    pub gate_operator_surface: ResolvedItemId,
    pub service_ramp_solids: Vec<ResolvedItemId>,
    pub route_nodes: Vec<ArtilleryRouteNode>,
    pub route_edges: Vec<ArtilleryRouteEdge>,
    pub retained_keep_setback_metres: f32,
    pub support_interfaces: Vec<ResolvedItemId>,
    pub drainage_routes: Vec<ResolvedItemId>,
}
