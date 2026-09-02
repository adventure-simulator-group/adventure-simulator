use super::*;

/// Visible plaster finish offset from the exposed timber face.
pub(crate) const TIMBER_INFILL_FINISH_SETBACK_METRES: f32 = 0.008;
/// Facade-space overlap of plaster beneath timber and opening trim.
pub(crate) const TIMBER_INFILL_EDGE_UNDERLAP_METRES: f32 = 0.006;

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct TimberFrameAssemblyId(pub u64);

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct TimberFacadeId(pub u64);

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct TimberFrameLineId(pub u64);

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct TimberStoreyFrameId(pub u64);

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct TimberFrameBayId(pub u64);

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct TimberMemberId(pub u64);

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct TimberJointId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimberFrameProgramKind {
    NarrowUrbanTownHouse,
    NorthernTwoPostHallHouse,
    DirectRoofCottage,
    JettiedMerchantHouse,
    CivicMasonryTimberHall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimberFramePhase {
    PrimaryConstruction,
    UpperStoreyAddition,
    RoofConstruction,
    NonStructuralFinish,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuralTimberMaterial {
    Oak,
    Fir,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimberStoreyKind {
    GroundFrame,
    UpperFrame,
    StorageAttic,
    CivicTimberHall,
    MasonryPlinth,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimberMemberRole {
    Sill,
    PrimaryPost,
    CornerPost,
    IntermediatePost,
    WallPlate,
    Rail,
    FloorJoist,
    TransverseTie,
    Girder,
    HeadBrace,
    FootBrace,
    StoreyBrace,
    JettyBeam,
    Knagge,
    GableTie,
    GablePost,
    Rafter,
    Collar,
    Purlin,
    DormerTrimmer,
    Ornament,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimberJointKind {
    FoundationBearing,
    MortiseTenon,
    HousedBeam,
    Scarf,
    Bridle,
    Lap,
    RoofSeat,
    JettyBearing,
    NonStructuralFixing,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimberFrameMember {
    pub id: TimberMemberId,
    pub owner: GeometryOwnerId,
    pub role: TimberMemberRole,
    pub phase: TimberFramePhase,
    /// Species/grade authority is member-local because later repairs and
    /// upper-storey additions may legitimately differ from the primary frame.
    pub material: StructuralTimberMaterial,
    pub start_node: StructuralNodeId,
    pub end_node: StructuralNodeId,
    pub start_joint: TimberJointId,
    pub end_joint: TimberJointId,
    pub start: Vec3,
    pub end: Vec3,
    pub section_metres: Vec2,
    pub solid: ResolvedItemId,
    pub support_interfaces: [ResolvedItemId; 2],
    pub structural: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct TimberJointParticipant {
    pub member: TimberMemberId,
    /// Member axis directed away from the joint contact in world space.
    pub axis_from_joint: Vec3,
    /// Equal and opposite contact reaction carried by the joint. Keeping both
    /// vectors explicit makes local-frame/cardinal mistakes machine-testable.
    pub reaction_direction: Vec3,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimberFrameJoint {
    pub id: TimberJointId,
    pub node: StructuralNodeId,
    pub kind: TimberJointKind,
    pub member_ids: Vec<TimberMemberId>,
    /// Exact endpoint contact patches participating in this joint. A type
    /// label without these physical counterparts is not a construction joint.
    pub contact_interfaces: Vec<ResolvedItemId>,
    /// Per-member action/reaction authority derived from the actual endpoint
    /// axes, never from a fixed world-space decorative convention.
    #[serde(default)]
    pub participants: Vec<TimberJointParticipant>,
    /// Principal carried-load direction in world space, used to reject a
    /// nominal joint type on geometrically incompatible participants.
    pub load_direction: Vec3,
    pub contact_area_square_metres: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimberFrameBay {
    pub id: TimberFrameBayId,
    pub wall: Option<WallAssemblyId>,
    pub opening: Option<OpeningAssemblyId>,
    pub member_ids: Vec<TimberMemberId>,
    pub infill_solids: Vec<ResolvedItemId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimberJettyAssembly {
    pub projection_metres: f32,
    pub backspan_metres: f32,
    pub jetty_beams: Vec<TimberMemberId>,
    pub knaggen: Vec<TimberMemberId>,
    pub corner_supports: Vec<TimberMemberId>,
    /// Authoritative upper-storey floor plate carried by the cantilever and
    /// its backspan. This is not decorative proof geometry.
    pub floor_solid: ResolvedItemId,
    pub floor_bearing_interfaces: Vec<ResolvedItemId>,
    pub support_polygon: Vec<Vec2>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimberFloorAssembly {
    pub level: u16,
    pub floor_solid: ResolvedItemId,
    pub floor_solids: Vec<ResolvedItemId>,
    pub route_surface: ResolvedItemId,
    pub girder_members: Vec<TimberMemberId>,
    pub joist_members: Vec<TimberMemberId>,
    pub bearing_interfaces: Vec<ResolvedItemId>,
    /// Sampled floor-to-joist contacts and joist-to-girder housed bearings.
    pub floor_joist_interfaces: Vec<ResolvedItemId>,
    pub joist_girder_interfaces: Vec<ResolvedItemId>,
    pub stair_connection: Option<Vec2>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimberRouteNodeKind {
    ExteriorApproach,
    DoorThreshold,
    GroundFloor,
    StairTread,
    Landing,
    UpperFloor,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimberRouteNode {
    pub surface: ResolvedItemId,
    pub kind: TimberRouteNodeKind,
    pub position: Vec3,
    pub level: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimberRouteEdge {
    pub from: ResolvedItemId,
    pub to: ResolvedItemId,
    pub clear_width_metres: f32,
    pub clear_headroom_metres: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimberCirculationAssembly {
    pub entry_opening: Option<OpeningAssemblyId>,
    pub nodes: Vec<TimberRouteNode>,
    pub edges: Vec<TimberRouteEdge>,
    pub stair_solids: Vec<ResolvedItemId>,
    pub landing_solids: Vec<ResolvedItemId>,
    pub floor_cut_voids: Vec<ResolvedItemId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimberStoreyFrame {
    pub id: TimberStoreyFrameId,
    pub level: u16,
    pub kind: TimberStoreyKind,
    pub base_elevation_metres: f32,
    pub top_elevation_metres: f32,
    pub bay_ids: Vec<TimberFrameBayId>,
    pub member_ids: Vec<TimberMemberId>,
    pub jetty: Option<TimberJettyAssembly>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimberFrameLine {
    pub id: TimberFrameLineId,
    pub origin: Vec2,
    pub tangent: Vec2,
    pub outward: Vec2,
    pub length_metres: f32,
    pub internal: bool,
    pub storeys: Vec<TimberStoreyFrame>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimberFrameFacade {
    pub id: TimberFacadeId,
    pub outward: Direction,
    pub lines: Vec<TimberFrameLine>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimberFrameAssembly {
    pub id: TimberFrameAssemblyId,
    pub program: TimberFrameProgramKind,
    pub phase: TimberFramePhase,
    pub material: StructuralTimberMaterial,
    pub facades: Vec<TimberFrameFacade>,
    pub internal_lines: Vec<TimberFrameLine>,
    pub bays: Vec<TimberFrameBay>,
    pub members: Vec<TimberFrameMember>,
    pub joints: Vec<TimberFrameJoint>,
    pub floors: Vec<TimberFloorAssembly>,
    pub circulation: TimberCirculationAssembly,
    /// Measured sill-to-masonry bearing contacts for the civic hybrid program.
    pub masonry_bearing_interfaces: Vec<ResolvedItemId>,
    pub roof_bearing_interfaces: Vec<ResolvedItemId>,
    pub dormer_trimmer_members: Vec<TimberMemberId>,
}
