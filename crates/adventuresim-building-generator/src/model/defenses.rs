use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrownMaterial {
    Masonry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrownPhase {
    PermanentMainWork,
    InnerKeep,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrownPattern {
    Crenellated,
    PiercedCrenellated,
    GunLoopParapet,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InnerEdgeTreatment {
    MasonryUpstand,
    GuardRail,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct CrownProfile {
    pub breastwork_height_metres: f32,
    pub merlon_height_metres: f32,
    pub thickness_metres: f32,
    pub merlon_width_metres: f32,
    pub crenel_width_metres: f32,
    pub coping_height_metres: f32,
    pub inner_guard_height_metres: f32,
    pub walk_clear_width_metres: f32,
    pub stance_height_metres: f32,
    pub firing_height_metres: f32,
    pub drain_spacing_metres: f32,
    pub inner_edge: InnerEdgeTreatment,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CrownPath {
    Straight {
        start: Vec2,
        end: Vec2,
        outward: Direction,
    },
    Round {
        tower_index: usize,
        centre: Vec2,
        radius_metres: f32,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrownJunctionKind {
    Corner,
    TowerSplice,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct CrownJunction {
    pub owner: GeometryOwnerId,
    pub other_owner: GeometryOwnerId,
    pub position: Vec2,
    pub kind: CrownJunctionKind,
    pub clear_width_metres: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrownAssembly {
    pub owner: GeometryOwnerId,
    pub path: CrownPath,
    pub base_height_metres: f32,
    pub profile: CrownProfile,
    pub material: CrownMaterial,
    pub phase: CrownPhase,
    pub pattern: CrownPattern,
    pub junctions: Vec<CrownJunction>,
    pub drain_positions: Vec<Vec2>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct CurtainWallRun {
    pub start: Vec2,
    pub end: Vec2,
    pub height_metres: f32,
    pub thickness_metres: f32,
    pub outward: Direction,
    pub gate_width_metres: Option<f32>,
    pub gate_height_metres: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum WallWalk {
    Linear {
        start: Vec2,
        end: Vec2,
        elevation_metres: f32,
        width_metres: f32,
        outward: Direction,
    },
    Round {
        centre: Vec2,
        elevation_metres: f32,
        outer_radius_metres: f32,
        stairwell_radius_metres: f32,
    },
    RectangularDeck {
        centre: Vec2,
        size: Vec2,
        elevation_metres: f32,
        stairwell_centre: Vec2,
        stairwell_size: Vec2,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefensiveJunctionKind {
    LevelLanding,
    Steps { riser_count: u8 },
}

/// A deliberately constructed connection between two fighting surfaces.
///
/// Merely overlapping rendered meshes is not enough to establish circulation:
/// this object records the usable landing or short flight at the junction.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct DefensiveJunction {
    pub walk_a: usize,
    pub walk_b: usize,
    pub centre: Vec2,
    pub width_metres: f32,
    pub clear_height_metres: f32,
    pub kind: DefensiveJunctionKind,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DefensiveCircuit {
    pub label: String,
    pub walks: Vec<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TowerPortalKind {
    GroundStairEntrance,
    WallWalkJunction { walk_index: usize },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct TowerPortal {
    pub tower_index: usize,
    pub facing: Direction,
    pub sill_elevation_metres: f32,
    pub width_metres: f32,
    pub clear_height_metres: f32,
    pub kind: TowerPortalKind,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct FiringPosition {
    pub aperture_id: u16,
    pub tower_index: usize,
    pub origin: Vec2,
    pub aperture_normal: Vec2,
    pub direction: Vec2,
    pub elevation_metres: f32,
    pub range_metres: f32,
    pub half_arc_degrees: f32,
    pub aperture_width_metres: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardOpeningKind {
    OutwardObservation,
    DownwardDefense,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct GuardChamberOpening {
    pub kind: GuardOpeningKind,
    pub position: Vec2,
    pub sill_elevation_metres: f32,
    pub width_metres: f32,
    pub clear_height_metres: f32,
    pub facing: Direction,
    pub target: Vec2,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct GuardChamberSupport {
    pub centre: Vec2,
    pub size: Vec2,
    pub base_elevation_metres: f32,
    pub top_elevation_metres: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct AccessLanding {
    pub centre: Vec2,
    pub size: Vec2,
    pub elevation_metres: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct AccessGuardSegment {
    pub start: Vec2,
    pub end: Vec2,
    pub elevation_metres: f32,
    pub height_metres: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct AccessBrace {
    pub start: Vec2,
    pub start_elevation_metres: f32,
    pub end: Vec2,
    pub end_elevation_metres: f32,
    pub thickness_metres: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct AccessLedger {
    pub centre: Vec2,
    pub size: Vec2,
    pub elevation_metres: f32,
    pub height_metres: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct AccessStairFlight {
    pub top: Vec2,
    pub bottom: Vec2,
    pub top_elevation_metres: f32,
    pub bottom_elevation_metres: f32,
    pub riser_count: u16,
    pub going_metres: f32,
    pub nosing_metres: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct AccessDoor {
    pub position: Vec2,
    pub facing: Direction,
    pub threshold_elevation_metres: f32,
    pub width_metres: f32,
    pub clear_height_metres: f32,
    pub swing_inward: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct TraversalEnvelope {
    pub width_metres: f32,
    pub height_metres: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GuardChamberAccess {
    pub from_walk_index: usize,
    pub envelope: TraversalEnvelope,
    pub top_landing: AccessLanding,
    pub flight: AccessStairFlight,
    pub bottom_landing: AccessLanding,
    pub top_walk_opening: AccessDoor,
    pub door: AccessDoor,
    pub roof_clearance_opening: AccessLanding,
    pub support_posts: Vec<GuardChamberSupport>,
    pub landing_guards: Vec<AccessGuardSegment>,
    pub flight_guard_height_metres: f32,
    pub wall_ledger: AccessLedger,
    pub lateral_braces: Vec<AccessBrace>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct GateOperatingPosition {
    pub closure_index: usize,
    pub position: Vec2,
    pub elevation_metres: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GateGuardChamber {
    pub centre: Vec2,
    pub size: Vec2,
    pub floor_elevation_metres: f32,
    pub clear_height_metres: f32,
    pub supporting_wall_index: usize,
    pub supports: Vec<GuardChamberSupport>,
    pub access: GuardChamberAccess,
    pub openings: Vec<GuardChamberOpening>,
    pub operating_positions: Vec<GateOperatingPosition>,
    pub load_path: GatehouseLoadPath,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum GatehouseLoadPath {
    BondedTowerBearing {
        left_tower_index: usize,
        right_tower_index: usize,
        bearing_depth: GridLength,
        arch_centre: Vec2,
        arch_spring_elevation_metres: f32,
        arch_ring_depth: GridLength,
        arch_rise: GridLength,
        curtain_return_bond: GridLength,
    },
}

/// Grid-native source of truth for a wall-local defended gate module.
///
/// Horizontal dimensions are project choices expressed on the 1/30-cell
/// structural lattice. World positions, towers, chamber, closures and firing
/// geometry are derived from the referenced cardinal curtain wall.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GatehouseAssemblySpec {
    pub curtain_wall_index: usize,
    pub gate_width: GridLength,
    pub tower_diameter: CellDiameter,
    pub tower_shell: GridLength,
    pub jamb_reveal: GridLength,
    pub chord_bearing: GridLength,
    pub chamber_depth: GridLength,
    pub arch_ring_depth: GridLength,
    pub arch_rise: GridLength,
    pub curtain_return_bond: GridLength,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateClosureKind {
    HeavyLeaves,
    Portcullis,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct GatePassageProfile {
    pub width_metres: f32,
    pub spring_height_metres: f32,
    pub arch_rise_metres: f32,
}

impl GatePassageProfile {
    pub fn height_at(self, along_metres: f32) -> f32 {
        let half = self.width_metres * 0.5;
        if half <= 0.0 || along_metres.abs() > half {
            return 0.0;
        }
        let normalized = along_metres / half;
        self.spring_height_metres + self.arch_rise_metres * (1.0 - normalized * normalized)
    }

    pub fn crown_height(self) -> f32 {
        self.spring_height_metres + self.arch_rise_metres
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct GateClosure {
    pub curtain_wall_index: usize,
    pub kind: GateClosureKind,
    pub inward_offset_metres: f32,
    pub coverage: GatePassageProfile,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GateDefense {
    pub curtain_wall_index: usize,
    pub threshold: Vec2,
    pub approach: Vec2,
    pub passage_profile: GatePassageProfile,
    pub firing_positions: Vec<FiringPosition>,
    pub closures: Vec<GateClosure>,
    pub guard_chamber: GateGuardChamber,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Bartizan {
    pub centre: Vec2,
    pub base_height_metres: f32,
    pub radius_metres: f32,
    pub height_metres: f32,
    pub roofed: bool,
}
