use super::*;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum WallSourceId {
    StoreyWall {
        storey_level: u16,
        wall_index: usize,
    },
    CurtainWall {
        wall_index: usize,
    },
    RoundTower {
        tower_index: usize,
    },
    ArtilleryCurtain {
        curtain_index: usize,
    },
    ArtilleryRondel {
        rondel_index: usize,
        station_index: usize,
    },
    SquareTowerFace {
        tower_index: usize,
        face: Direction,
        bay: u8,
    },
    CathedralClerestory {
        side: Direction,
    },
    RoofChildFront {
        roof: RoofAssemblyId,
    },
    ChurchExterior {
        range: ChurchRange,
        side: Direction,
        bay: u8,
    },
    ChurchArcade {
        side: Direction,
        bay: u8,
    },
    ChurchCrossing {
        side: Direction,
    },
    ChurchApse {
        facet: u8,
    },
    ChurchTowerFace {
        face: Direction,
        stage: ChurchTowerStage,
        bay: u8,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChurchRange {
    Nave,
    Transept,
    Choir,
    Apse,
    WestTower,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChurchTowerStage {
    Portal,
    Stair,
    Bell,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallMaterialClass {
    TimberInfill,
    CivilianMasonry,
    CathedralMasonry,
    FortifiedMasonry,
    InternalTimber,
    InternalMasonry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallStructuralRole {
    Infill,
    LoadBearing,
    Buttressed,
    Curtain,
    TowerShell,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct WallLocalFrame {
    pub origin: Vec2,
    pub tangent: Vec2,
    pub outward: Vec2,
    pub inside_room: Option<u16>,
    pub outside_room: Option<u16>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct RadialWallFrame {
    pub centre: Vec2,
    /// Deterministic radial axis used by section proofs and opening stations.
    pub reference_outward: Vec2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpeningUse {
    Door,
    Window,
    Gate,
    ArrowLoop,
    GunLoop,
    BellOpening,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaponMountClass {
    Handgun,
    LightArquebus,
    LightSwivelGun,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum OpeningProfile {
    Rectangular {
        width_metres: f32,
        height_metres: f32,
    },
    Segmental {
        width_metres: f32,
        spring_height_metres: f32,
        rise_metres: f32,
        intrados_depth_metres: f32,
    },
    PointedTwoCentred {
        width_metres: f32,
        spring_height_metres: f32,
        apex_height_metres: f32,
        arc_radius_metres: f32,
    },
    ArrowLoop {
        exterior_width_metres: f32,
        interior_width_metres: f32,
        exterior_height_metres: f32,
        interior_height_metres: f32,
    },
    GunLoop {
        exterior_width_metres: f32,
        interior_width_metres: f32,
        exterior_height_metres: f32,
        interior_height_metres: f32,
        mount: WeaponMountClass,
        traverse_degrees: f32,
        recoil_metres: f32,
        crew_clearance_metres: f32,
    },
}

impl OpeningProfile {
    pub fn exterior_width_metres(self) -> f32 {
        match self {
            Self::Rectangular { width_metres, .. }
            | Self::Segmental { width_metres, .. }
            | Self::PointedTwoCentred { width_metres, .. } => width_metres,
            Self::ArrowLoop {
                exterior_width_metres,
                ..
            }
            | Self::GunLoop {
                exterior_width_metres,
                ..
            } => exterior_width_metres,
        }
    }

    pub fn interior_width_metres(self) -> f32 {
        match self {
            Self::Rectangular { width_metres, .. }
            | Self::Segmental { width_metres, .. }
            | Self::PointedTwoCentred { width_metres, .. } => width_metres,
            Self::ArrowLoop {
                interior_width_metres,
                ..
            }
            | Self::GunLoop {
                interior_width_metres,
                ..
            } => interior_width_metres,
        }
    }

    pub fn clear_height_metres(self) -> f32 {
        match self {
            Self::Rectangular { height_metres, .. } => height_metres,
            Self::Segmental {
                spring_height_metres,
                rise_metres,
                ..
            } => spring_height_metres + rise_metres,
            Self::PointedTwoCentred {
                apex_height_metres, ..
            } => apex_height_metres,
            Self::ArrowLoop {
                interior_height_metres,
                ..
            }
            | Self::GunLoop {
                interior_height_metres,
                ..
            } => interior_height_metres,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpeningHeadKind {
    TimberLintel,
    StoneLintel,
    SegmentalArch,
    PointedVoussoir,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClosureKind {
    OpenMilitary,
    TimberShutter,
    LeadedGlazing,
    OiledClothLattice,
    DoorLeaf,
    TimberLouvre,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClosureState {
    Open,
    Closed,
    Operable,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClosurePolicy {
    pub layers: Vec<ClosureKind>,
    pub state: ClosureState,
    pub thickness_metres: f32,
    pub swing_clearance_metres: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WallAssembly {
    pub id: WallAssemblyId,
    pub owner: GeometryOwnerId,
    pub source: WallSourceId,
    pub material: WallMaterialClass,
    pub storey_level: u16,
    pub frame: WallLocalFrame,
    pub radial_frame: Option<RadialWallFrame>,
    pub length_metres: f32,
    pub height_metres: f32,
    pub base_elevation_metres: f32,
    pub thickness_metres: f32,
    pub structural_role: WallStructuralRole,
    pub support_node: StructuralNodeId,
    pub host_solids: Vec<ResolvedItemId>,
    pub opening_ids: Vec<OpeningAssemblyId>,
    pub replaced_by_owner: Option<GeometryOwnerId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WallStyleOverride {
    pub wall: WallSelector,
    pub style: WallStyle,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpeningAssembly {
    pub id: OpeningAssemblyId,
    pub owner: GeometryOwnerId,
    pub host_wall: WallAssemblyId,
    pub host_source: WallSourceId,
    pub frame: WallLocalFrame,
    pub use_kind: OpeningUse,
    pub profile: OpeningProfile,
    pub sill_elevation_metres: f32,
    pub closure: ClosurePolicy,
    pub head_kind: OpeningHeadKind,
    pub void_id: ResolvedItemId,
    pub jamb_solids: [ResolvedItemId; 2],
    pub sill_solid: Option<ResolvedItemId>,
    pub head_solid: ResolvedItemId,
    pub spandrel_solid: ResolvedItemId,
    pub reveal_surfaces: Vec<ResolvedItemId>,
    pub closure_solids: Vec<ResolvedItemId>,
    pub jamb_nodes: [StructuralNodeId; 2],
    pub head_node: StructuralNodeId,
    pub spandrel_node: StructuralNodeId,
    pub tracery_node: Option<StructuralNodeId>,
    pub stance_surface: Option<ResolvedItemId>,
    pub mount_solid: Option<ResolvedItemId>,
    pub ray_indices: Vec<usize>,
    /// Ordered free-space samples from the exterior throat (0) to the
    /// interior mouth (1). These are the subtraction authority; the broad
    /// bounds on `void_id` are only its conservative envelope.
    pub sectional_void: Vec<OpeningVoidSlice>,
    pub head_bearing_interfaces: [ResolvedItemId; 2],
    pub wall_above_interface: ResolvedItemId,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct OpeningVoidSlice {
    pub depth_fraction: f32,
    pub width_metres: f32,
    pub height_metres: f32,
}
