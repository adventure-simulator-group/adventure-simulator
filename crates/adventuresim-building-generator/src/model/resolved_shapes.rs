use super::*;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct GeometryOwnerId(pub u32);

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct ResolvedItemId(pub u64);

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct WallAssemblyId(pub u64);

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct OpeningAssemblyId(pub u64);

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct StructuralNodeId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuralNodeKind {
    WallBearing,
    TowerShellBearing,
    ProjectionCorbel,
    GalleryFrame,
    OpeningJamb,
    OpeningHead,
    OpeningSpandrel,
    MullionBearing,
    ButtressBearing,
    RoofWallPlate,
    RoofRafter,
    RoofRidgePurlin,
    RoofHipRafter,
    RoofValleyRafter,
    RoofTrimmer,
    RoofTowerRing,
    ChurchPier,
    ChurchArcadeSpringing,
    ChurchVaultSpringing,
    ChurchCrossingPier,
    ChurchButtress,
    ChurchTowerStage,
    ChurchBellFrame,
    TimberFrameFoundation,
    TimberFrameJoint,
    TimberFrameStoreyBearing,
    TimberFrameJettyBearing,
    TimberFrameRoofBearing,
    ArtilleryRevetmentBearing,
    ArtilleryRetainingBearing,
    ArtilleryTerrepleinBearing,
    ArtilleryRondelBearing,
    ArtilleryBridgeAbutment,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolidRole {
    LoadBearing,
    Breastwork,
    Merlon,
    Coping,
    EdgeGuard,
    WalkSurface,
    DrainageChannel,
    GalleryFloor,
    ProjectionSupport,
    DefenseWall,
    DefenseRoof,
    FrameMember,
    Landing,
    BartizanShell,
    DefenseHostWall,
    DefenseHostButtress,
    CircuitWalk,
    BeamJoist,
    DrainageFloor,
    RoofFlashing,
    RoofPlate,
    WallHost,
    OpeningJamb,
    OpeningSill,
    OpeningHead,
    OpeningSpandrel,
    OpeningReveal,
    OpeningClosure,
    LeadedGlazing,
    WeaponMount,
    Mullion,
    WallButtress,
    RoofFace,
    RoofFraming,
    RoofEdgeTreatment,
    RoofGutter,
    ChurchFloor,
    ChurchPier,
    ChurchArcade,
    ChurchVaultShell,
    ChurchVaultThrust,
    ChurchCrossingArch,
    ChurchBellFloor,
    ChurchBellFrame,
    ChurchBell,
    ChurchGuard,
    ChurchStairNewel,
    ChurchStairTread,
    ChurchServiceLadder,
    FrameSill,
    FramePost,
    FramePlate,
    FrameRail,
    FrameJoist,
    FrameGirder,
    FrameTie,
    FrameBrace,
    FrameJettyBeam,
    FrameKnagge,
    FrameFloor,
    FrameGableMember,
    FrameDormerTrimmer,
    FrameInfill,
    FrameOrnament,
    ArtilleryRevetment,
    ArtilleryEarthCore,
    ArtilleryRetainingWall,
    ArtilleryTerreplein,
    ArtilleryParapet,
    ArtilleryStairGuard,
    ArtilleryCasemateFloor,
    ArtilleryCasemateRoof,
    ArtilleryRamp,
    ArtilleryStairTread,
    ArtilleryBridgeAbutment,
    ArtilleryBridgeBeam,
    ArtilleryBridgeDeck,
    ArtilleryGateMechanism,
    DitchScarp,
    DitchCounterscarp,
    DitchFloor,
}

/// Project gate: crowns reserve this much of the exposed walk edge for a
/// recessed, open drainage slot. This is a gameplay/readability dimension,
/// not a claimed universal historical measurement.
pub const CROWN_DRAIN_CHANNEL_WIDTH_METRES: f32 = 0.12;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceRole {
    Stance,
    Drainage,
    FiringLine,
    ProjectedWalk,
    DefenseFloor,
    WeatherSill,
    Intrados,
    LeftJambReveal,
    RightJambReveal,
    ExteriorThroat,
    InteriorMouth,
    RoofWeatherSurface,
    RoofDrainage,
    DrainageRecipient,
    ChurchPublicRoute,
    ChurchServiceRoute,
    ChurchVaultLoad,
    TimberCirculation,
    ArtilleryRoute,
    ArtilleryStance,
    ArtilleryDrainage,
    DitchDrainage,
    DitchSplash,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ResolvedSurfaceShape {
    Planar,
    RouteCorridor {
        start: Vec3,
        end: Vec3,
        width_metres: f32,
    },
    SplayedJamb {
        side: i8,
        exterior_width_metres: f32,
        interior_width_metres: f32,
        exterior_depth_sign: i8,
    },
    WeatherSill {
        interior_elevation_metres: f32,
        exterior_elevation_metres: f32,
        drip_depth_metres: f32,
    },
    SegmentalIntrados {
        clear_span_metres: f32,
        spring_height_metres: f32,
        rise_metres: f32,
    },
    PointedIntrados {
        clear_span_metres: f32,
        spring_height_metres: f32,
        apex_height_metres: f32,
        arc_radius_metres: f32,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoidRole {
    Crenel,
    Drain,
    Passage,
    DefenseThroat,
    AccessPortal,
    FiringAperture,
    BeamSocket,
    WallOpening,
    RoofOpening,
    ArtilleryCasemate,
    ArtillerySmokeVent,
    DryDitch,
    BridgeDeniedGap,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ResolvedSolidShape {
    Cuboid,
    SegmentalArchRing {
        clear_span_metres: f32,
        spring_height_metres: f32,
        rise_metres: f32,
        ring_depth_metres: f32,
    },
    PointedArchRing {
        clear_span_metres: f32,
        spring_height_metres: f32,
        apex_height_metres: f32,
        arc_radius_metres: f32,
        ring_depth_metres: f32,
    },
    SplayedReveal {
        exterior_width_metres: f32,
        interior_width_metres: f32,
        side: i8,
        /// Sign of the exterior face on the resolved shape's local depth axis.
        exterior_depth_sign: i8,
    },
    SplayedHead {
        exterior_clear_height_metres: f32,
        interior_clear_height_metres: f32,
        /// Sign of the exterior face on the resolved shape's local depth axis.
        exterior_depth_sign: i8,
    },
    /// One exact triangular prism in a wall-local Gefach partition. The
    /// vertices lie on the infill mid-plane in world space; `outward` and
    /// `depth_metres` extrude the closed panel through its authoritative
    /// thickness. Triangles let the resolver subtract diagonal braces without
    /// falling back to a continuous backing sheet.
    TimberPanelPrism {
        vertices: [Vec3; 3],
        outward: Vec2,
        depth_metres: f32,
    },
    RoundTowerShell {
        outer_radius_metres: f32,
        inner_radius_metres: f32,
        chord_interfaces: [Option<TowerChordInterface>; 2],
    },
    /// A closed annular prism. This is the authoritative rondel earth/deck
    /// primitive; an AABB would project through the circular revetment at its
    /// corners.
    AnnularPrism {
        inner_radius_metres: f32,
        outer_radius_metres: f32,
        inner_top_offset_metres: f32,
        outer_top_offset_metres: f32,
        drainage_outlet_count: u8,
        circumferential_fall_metres: f32,
    },
    /// A closed annular wedge used where a rondel ring is interrupted by
    /// authoritative rooms, galleries, portals, embrasures, or drains.
    /// Angles are in world-plan radians and increase counter-clockwise.
    AnnularSectorPrism {
        inner_radius_metres: f32,
        outer_radius_metres: f32,
        start_angle_radians: f32,
        end_angle_radians: f32,
        inner_top_offset_metres: f32,
        outer_top_offset_metres: f32,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ResolvedVoidShape {
    Box,
    /// A below-grade rectangular ditch band. The outer boundary is the
    /// void's bounds; this inner court-side boundary prevents the ditch from
    /// falsely claiming the castle interior as excavated free space.
    RectangularRing {
        inner_min: Vec2,
        inner_max: Vec2,
    },
    SectionalOpening {
        opening: OpeningAssemblyId,
        exterior_width_metres: f32,
        interior_width_metres: f32,
        exterior_height_metres: f32,
        interior_height_metres: f32,
        exterior_depth_sign: i8,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ResolvedBounds {
    pub min: Vec3,
    pub max: Vec3,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolvedSolid {
    pub id: ResolvedItemId,
    pub owner: GeometryOwnerId,
    pub centre: Vec3,
    pub size: Vec3,
    pub yaw_radians: f32,
    pub crossfall_radians: f32,
    pub longfall_radians: f32,
    pub role: SolidRole,
    pub shape: ResolvedSolidShape,
    pub supported_by: Vec<StructuralNodeId>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ResolvedSurface {
    pub id: ResolvedItemId,
    pub owner: GeometryOwnerId,
    pub bounds: ResolvedBounds,
    pub role: SurfaceRole,
    pub shape: ResolvedSurfaceShape,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ResolvedVoid {
    pub id: ResolvedItemId,
    pub owner: GeometryOwnerId,
    pub bounds: ResolvedBounds,
    pub role: VoidRole,
    pub shape: ResolvedVoidShape,
    pub subtracts_from: GeometryOwnerId,
}
