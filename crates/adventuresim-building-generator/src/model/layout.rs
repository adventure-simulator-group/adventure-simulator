use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Room {
    pub id: u16,
    pub kind: RoomKind,
    pub cells: Vec<Cell>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct WallSegment {
    pub cell: Cell,
    pub direction: Direction,
    pub inside_room: u16,
    pub outside_room: Option<u16>,
}

impl WallSegment {
    pub fn centre(self) -> Vec2 {
        let centre = self.cell.centre();
        let half = CELL_SIZE_METRES * 0.5;
        match self.direction {
            Direction::North => centre + Vec2::Y * half,
            Direction::East => centre + Vec2::X * half,
            Direction::South => centre - Vec2::Y * half,
            Direction::West => centre - Vec2::X * half,
        }
    }

    pub const fn is_horizontal(self) -> bool {
        matches!(self.direction, Direction::North | Direction::South)
    }

    pub const fn exterior(self) -> bool {
        self.outside_room.is_none()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpeningKind {
    Door,
    Window,
    Gate,
    ArrowSlit,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Opening {
    pub wall: usize,
    pub kind: OpeningKind,
    pub width_metres: f32,
    pub sill_metres: f32,
    pub height_metres: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoreyPlan {
    pub level: u16,
    pub rooms: Vec<Room>,
    pub walls: Vec<WallSegment>,
    pub openings: Vec<Opening>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct RoofPiece {
    pub kind: RoofKind,
    pub centre: Vec2,
    pub size: Vec2,
    pub base_height_metres: f32,
    pub pitch_degrees: f32,
    pub ridge_axis: RidgeAxis,
    pub eave_metres: f32,
    pub gable_profile: GableProfile,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct RoofDormer {
    pub centre: Vec2,
    pub base_height_metres: f32,
    pub width_metres: f32,
    pub depth_metres: f32,
    pub height_metres: f32,
    pub facing: Direction,
    pub kind: DormerKind,
    pub gable_profile: GableProfile,
}

/// Stable authority for one connected roof graph.  The old `RoofPiece` and
/// `RoofDormer` values are input recipes only; accepted plans render and audit
/// these assemblies instead.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct RoofAssemblyId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoofMaterial {
    ClayTile,
    Slate,
    Lead,
    TimberShingle,
    TimberInfill,
    MasonryInfill,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoofPhase {
    Primary,
    AttachedChild,
    LaterAddition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoofEdgeKind {
    Ridge,
    Hip,
    Valley,
    Eave,
    GableVerge,
    WallAbutment,
    TowerAbutment,
    OpeningCut,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoofPivotPolicy {
    KeepEave,
    KeepRidge,
    KeepChildAttachment,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoofEditError {
    MissingAssembly,
    PitchOutsideProjectRange,
    TopologyEvent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoofChildKind {
    GabledDormer,
    ShedDormer,
    CrossGable,
    Tower,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoofFootprintLoop {
    pub vertices: Vec<GridPoint>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct RoofPlaneEquation {
    pub normal: Vec3,
    pub constant: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoofFace {
    pub id: ResolvedItemId,
    pub polygon: Vec<Vec3>,
    /// Ordered holes cut out of this weather face by child assemblies or
    /// tower/wall abutments.  Winding is opposite the outer polygon.
    pub cutouts: Vec<Vec<Vec3>>,
    pub plane: RoofPlaneEquation,
    pub pitch_degrees: f32,
    pub thickness_metres: f32,
    pub material: RoofMaterial,
    pub support_nodes: Vec<StructuralNodeId>,
    pub drainage_catchment: ResolvedItemId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoofEnclosureFace {
    pub id: ResolvedItemId,
    pub polygon: Vec<Vec3>,
    pub material: RoofMaterial,
    pub support_nodes: Vec<StructuralNodeId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoofEdge {
    pub id: ResolvedItemId,
    pub start: Vec3,
    pub end: Vec3,
    pub kind: RoofEdgeKind,
    /// Boundary edges own one face; internal graph edges own exactly two.
    pub adjacent_faces: Vec<ResolvedItemId>,
    pub flashing: Option<ResolvedItemId>,
    pub drainage_terminal: Option<ResolvedItemId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoofChildAssembly {
    pub child: RoofAssemblyId,
    pub kind: RoofChildKind,
    pub parent_cut: ResolvedItemId,
    pub trimmer_nodes: Vec<StructuralNodeId>,
    pub valley_edges: Vec<ResolvedItemId>,
    pub flashing_ids: Vec<ResolvedItemId>,
    /// A Zwerchhaus is grounded in the facade rather than merely perched in
    /// a parent roof cut. Ordinary dormers leave this unset.
    pub facade_wall: Option<WallAssemblyId>,
    /// Ordered left-eave, facade opening-cut, right-eave edges replacing the
    /// continuous parent eave at a facade-derived cross gable.
    pub split_eave_edges: Vec<ResolvedItemId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoofAbutmentKind {
    Wall,
    Tower,
}

/// One measured station around a roof-to-masonry contact contour.  Stations
/// are spaced closely enough that their overlapping weathering pieces form a
/// continuous physical upstand instead of a symbolic strip spanning daylight.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoofAbutmentSample {
    pub point: Vec3,
    pub host_wall: WallAssemblyId,
    pub apron_solid: ResolvedItemId,
    pub upstand_solid: ResolvedItemId,
    pub counterflashing_solid: ResolvedItemId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoofAbutmentAssembly {
    pub id: ResolvedItemId,
    pub kind: RoofAbutmentKind,
    pub edge_ids: Vec<ResolvedItemId>,
    pub samples: Vec<RoofAbutmentSample>,
    pub lower_outlet: ResolvedItemId,
    pub drainage_route: ResolvedItemId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoofAssembly {
    pub id: RoofAssemblyId,
    pub owner: GeometryOwnerId,
    pub kind: RoofKind,
    pub outer_loop: RoofFootprintLoop,
    pub holes: Vec<RoofFootprintLoop>,
    pub faces: Vec<RoofFace>,
    pub enclosure_faces: Vec<RoofEnclosureFace>,
    pub edges: Vec<RoofEdge>,
    pub children: Vec<RoofChildAssembly>,
    pub abutments: Vec<RoofAbutmentAssembly>,
    pub parent: Option<RoofAssemblyId>,
    pub material: RoofMaterial,
    pub phase: RoofPhase,
    pub pivot_policy: RoofPivotPolicy,
    /// High side of a mono-pitch roof. `None` is required for roof kinds
    /// whose face graph already determines every slope direction.
    pub shed_high_side: Option<Direction>,
    pub support_nodes: Vec<StructuralNodeId>,
    pub source_piece_index: Option<usize>,
    pub source_tower_index: Option<usize>,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct RoundTower {
    anchor: GridPoint,
    diameter: CellDiameter,
    pub wall_height_metres: f32,
    pub wall_thickness_metres: f32,
    pub roof: Option<RoofPiece>,
    pub battlement: Option<BattlementKind>,
    pub chord_interface: Option<TowerChordInterface>,
    pub secondary_chord_interface: Option<TowerChordInterface>,
}

impl RoundTower {
    pub const fn new(
        anchor: GridPoint,
        diameter: CellDiameter,
        wall_height_metres: f32,
        wall_thickness_metres: f32,
        roof: Option<RoofPiece>,
        battlement: Option<BattlementKind>,
    ) -> Option<Self> {
        if !tower_anchor_matches_diameter(anchor, diameter) {
            return None;
        }
        Some(Self {
            anchor,
            diameter,
            wall_height_metres,
            wall_thickness_metres,
            roof,
            battlement,
            chord_interface: None,
            secondary_chord_interface: None,
        })
    }

    pub const fn anchor(self) -> GridPoint {
        self.anchor
    }

    pub const fn diameter(self) -> CellDiameter {
        self.diameter
    }

    pub fn centre_metres(self) -> Vec2 {
        self.anchor.metres()
    }

    pub fn radius_metres(self) -> f32 {
        self.diameter.metres() * 0.5
    }

    pub const fn with_chord_interface(mut self, interface: TowerChordInterface) -> Self {
        self.chord_interface = Some(interface);
        self
    }

    pub const fn with_secondary_chord_interface(mut self, interface: TowerChordInterface) -> Self {
        self.secondary_chord_interface = Some(interface);
        self
    }

    pub fn chord_interfaces(self) -> impl Iterator<Item = TowerChordInterface> {
        [self.chord_interface, self.secondary_chord_interface]
            .into_iter()
            .flatten()
    }
}

const fn tower_anchor_matches_diameter(anchor: GridPoint, diameter: CellDiameter) -> bool {
    let expected = if diameter.cells().is_multiple_of(2) {
        0
    } else {
        GRID_UNITS_PER_CELL / 2
    };
    anchor.x.rem_euclid(GRID_UNITS_PER_CELL) == expected
        && anchor.z.rem_euclid(GRID_UNITS_PER_CELL) == expected
}

#[derive(Deserialize)]
struct RoundTowerWire {
    anchor: GridPoint,
    diameter: CellDiameter,
    wall_height_metres: f32,
    wall_thickness_metres: f32,
    roof: Option<RoofPiece>,
    battlement: Option<BattlementKind>,
    chord_interface: Option<TowerChordInterface>,
    secondary_chord_interface: Option<TowerChordInterface>,
}

impl<'de> Deserialize<'de> for RoundTower {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RoundTowerWire::deserialize(deserializer)?;
        let mut tower = Self::new(
            wire.anchor,
            wire.diameter,
            wire.wall_height_metres,
            wire.wall_thickness_metres,
            wire.roof,
            wire.battlement,
        )
        .ok_or_else(|| de::Error::custom("tower anchor parity does not match its cell diameter"))?;
        tower.chord_interface = wire.chord_interface;
        tower.secondary_chord_interface = wire.secondary_chord_interface;
        Ok(tower)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TowerChordInterface {
    pub toward_gate: Direction,
    pub bearing_depth: GridLength,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SquareTower {
    pub centre: Vec2,
    pub size: Vec2,
    pub wall_height_metres: f32,
    pub roof: RoofPiece,
    pub bell_openings: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Stair {
    Straight {
        start: Vec2,
        direction: Direction,
        base_height_metres: f32,
        rise_metres: f32,
        width_metres: f32,
        tread_count: u16,
        /// The horizontal flight length.
        run_metres: f32,
    },
    Spiral {
        centre: Vec2,
        base_height_metres: f32,
        rise_metres: f32,
        inner_radius_metres: f32,
        outer_radius_metres: f32,
        turns: f32,
        clockwise: bool,
        tread_count: u16,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct BattlementRun {
    pub start: Vec2,
    pub end: Vec2,
    pub base_height_metres: f32,
    pub kind: BattlementKind,
    pub outward: Direction,
}
