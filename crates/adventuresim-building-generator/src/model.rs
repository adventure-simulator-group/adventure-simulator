use std::fmt;

use bevy::math::{IVec2, Vec2};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

pub const CELL_SIZE_METRES: f32 = 1.5;
pub const WALL_THICKNESS_METRES: f32 = 0.18;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, Serialize, Deserialize)]
pub struct Cell {
    pub x: i16,
    pub z: i16,
}

impl Cell {
    pub const fn new(x: i16, z: i16) -> Self {
        Self { x, z }
    }

    pub fn centre(self) -> Vec2 {
        Vec2::new(
            (f32::from(self.x) + 0.5) * CELL_SIZE_METRES,
            (f32::from(self.z) + 0.5) * CELL_SIZE_METRES,
        )
    }

    pub fn neighbour(self, direction: Direction) -> Self {
        let offset = direction.offset();
        Self::new(self.x + offset.x as i16, self.z + offset.y as i16)
    }
}

impl From<Cell> for IVec2 {
    fn from(cell: Cell) -> Self {
        Self::new(i32::from(cell.x), i32::from(cell.z))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    North,
    East,
    South,
    West,
}

impl Direction {
    pub const ALL: [Self; 4] = [Self::North, Self::East, Self::South, Self::West];

    pub const fn offset(self) -> IVec2 {
        match self {
            Self::North => IVec2::new(0, 1),
            Self::East => IVec2::new(1, 0),
            Self::South => IVec2::new(0, -1),
            Self::West => IVec2::new(-1, 0),
        }
    }

    pub const fn opposite(self) -> Self {
        match self {
            Self::North => Self::South,
            Self::East => Self::West,
            Self::South => Self::North,
            Self::West => Self::East,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomKind {
    EntranceHall,
    Passage,
    GreatHall,
    CommonRoom,
    Kitchen,
    Pantry,
    Workshop,
    Shop,
    Storage,
    Bedchamber,
    StairHall,
    Guardroom,
    Armoury,
    Chapel,
    Gallery,
    TowerChamber,
}

impl fmt::Display for RoomKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoomRequirement {
    pub kind: RoomKind,
    pub preferred_cells: u16,
    pub needs_exterior: bool,
    pub preferred_neighbours: Vec<RoomKind>,
}

impl RoomRequirement {
    pub fn new(kind: RoomKind, preferred_cells: u16) -> Self {
        Self {
            kind,
            preferred_cells,
            needs_exterior: false,
            preferred_neighbours: Vec::new(),
        }
    }

    pub fn exterior(mut self) -> Self {
        self.needs_exterior = true;
        self
    }

    pub fn beside(mut self, kind: RoomKind) -> Self {
        self.preferred_neighbours.push(kind);
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoreyProgram {
    pub rooms: Vec<RoomRequirement>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Footprint {
    Rectangle {
        width: u16,
        depth: u16,
    },
    Courtyard {
        width: u16,
        depth: u16,
        wing: u16,
        gate_width: u16,
    },
}

impl Footprint {
    pub const fn dimensions(self) -> (u16, u16) {
        match self {
            Self::Rectangle { width, depth } | Self::Courtyard { width, depth, .. } => {
                (width, depth)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallStyle {
    TimberFrame,
    Plaster,
    Brick,
    Stone,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimberFrameStyle {
    LateMedieval,
    NorthernCloseStudded,
    EarlyModernOrnate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoofKind {
    Gable,
    Hip,
    HalfHip,
    Shed,
    Flat,
    Pavilion,
    Conical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GableProfile {
    Plain,
    Stepped,
    Curved,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DormerKind {
    Gabled,
    Hipped,
    Shed,
    TransverseGable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RidgeAxis {
    X,
    Z,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BattlementKind {
    Crenellated,
    PiercedCrenellated,
    Machicolated,
    OpenHoarding,
    RoofedHoarding,
    CoveredWallWalk,
    GunLoopParapet,
    Breteche,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum BuildingArchetype {
    TownHouse,
    HallHouse,
    FachwerkMerchantHouse,
    RenaissanceTownHall,
    CastleGatehouse,
    CourtyardCastle,
}

impl BuildingArchetype {
    pub const ALL: [Self; 6] = [
        Self::TownHouse,
        Self::HallHouse,
        Self::FachwerkMerchantHouse,
        Self::RenaissanceTownHall,
        Self::CastleGatehouse,
        Self::CourtyardCastle,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::TownHouse => "town-house",
            Self::HallHouse => "hall-house",
            Self::FachwerkMerchantHouse => "fachwerk-merchant-house",
            Self::RenaissanceTownHall => "renaissance-town-hall",
            Self::CastleGatehouse => "castle-gatehouse",
            Self::CourtyardCastle => "courtyard-castle",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildingProgram {
    pub archetype: BuildingArchetype,
    pub seed: u64,
    pub footprint: Footprint,
    pub storey_height_metres: f32,
    pub storeys: Vec<StoreyProgram>,
    pub wall_style: WallStyle,
    pub timber_frame_style: Option<TimberFrameStyle>,
    pub upper_storey_projection_metres: f32,
    pub roof_pitch_degrees: f32,
}

impl BuildingProgram {
    pub fn fixture(archetype: BuildingArchetype, seed: u64) -> Self {
        use RoomKind::*;

        match archetype {
            BuildingArchetype::TownHouse => Self {
                archetype,
                seed,
                footprint: Footprint::Rectangle {
                    width: 6,
                    depth: 10,
                },
                storey_height_metres: 3.0,
                storeys: vec![
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(Shop, 18).exterior().beside(Workshop),
                            RoomRequirement::new(EntranceHall, 8)
                                .exterior()
                                .beside(StairHall),
                            RoomRequirement::new(Workshop, 15).beside(Storage),
                            RoomRequirement::new(Storage, 8),
                            RoomRequirement::new(StairHall, 8),
                        ],
                    },
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(CommonRoom, 22)
                                .exterior()
                                .beside(Kitchen),
                            RoomRequirement::new(Kitchen, 12).exterior().beside(Pantry),
                            RoomRequirement::new(Pantry, 6),
                            RoomRequirement::new(Bedchamber, 13).exterior(),
                            RoomRequirement::new(StairHall, 7),
                        ],
                    },
                ],
                wall_style: WallStyle::TimberFrame,
                timber_frame_style: Some(TimberFrameStyle::LateMedieval),
                upper_storey_projection_metres: 0.22,
                roof_pitch_degrees: 55.0,
            },
            BuildingArchetype::HallHouse => Self {
                archetype,
                seed,
                footprint: Footprint::Rectangle {
                    width: 9,
                    depth: 13,
                },
                storey_height_metres: 3.3,
                storeys: vec![StoreyProgram {
                    rooms: vec![
                        RoomRequirement::new(GreatHall, 52)
                            .exterior()
                            .beside(Kitchen),
                        RoomRequirement::new(EntranceHall, 14).exterior(),
                        RoomRequirement::new(Kitchen, 20).exterior().beside(Pantry),
                        RoomRequirement::new(Pantry, 10),
                        RoomRequirement::new(Storage, 15).exterior(),
                    ],
                }],
                wall_style: WallStyle::TimberFrame,
                timber_frame_style: Some(TimberFrameStyle::NorthernCloseStudded),
                upper_storey_projection_metres: 0.0,
                roof_pitch_degrees: 50.0,
            },
            BuildingArchetype::FachwerkMerchantHouse => Self {
                archetype,
                seed,
                footprint: Footprint::Rectangle {
                    width: 8,
                    depth: 11,
                },
                storey_height_metres: 3.0,
                storeys: vec![
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(Shop, 24).exterior().beside(Workshop),
                            RoomRequirement::new(EntranceHall, 10)
                                .exterior()
                                .beside(StairHall),
                            RoomRequirement::new(Workshop, 22).beside(Storage),
                            RoomRequirement::new(Storage, 16),
                            RoomRequirement::new(StairHall, 16),
                        ],
                    },
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(CommonRoom, 30)
                                .exterior()
                                .beside(Kitchen),
                            RoomRequirement::new(Kitchen, 18).exterior().beside(Pantry),
                            RoomRequirement::new(Pantry, 8),
                            RoomRequirement::new(Bedchamber, 20).exterior(),
                            RoomRequirement::new(StairHall, 12),
                        ],
                    },
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(Gallery, 26).exterior(),
                            RoomRequirement::new(Bedchamber, 24).exterior(),
                            RoomRequirement::new(Bedchamber, 20).exterior(),
                            RoomRequirement::new(Storage, 10),
                            RoomRequirement::new(StairHall, 8),
                        ],
                    },
                ],
                wall_style: WallStyle::TimberFrame,
                timber_frame_style: Some(TimberFrameStyle::EarlyModernOrnate),
                upper_storey_projection_metres: 0.28,
                roof_pitch_degrees: 57.0,
            },
            BuildingArchetype::RenaissanceTownHall => Self {
                archetype,
                seed,
                footprint: Footprint::Rectangle {
                    width: 14,
                    depth: 10,
                },
                storey_height_metres: 3.4,
                storeys: vec![
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(EntranceHall, 30).exterior(),
                            RoomRequirement::new(GreatHall, 48).exterior(),
                            RoomRequirement::new(Shop, 24).exterior(),
                            RoomRequirement::new(Storage, 18),
                            RoomRequirement::new(StairHall, 20),
                        ],
                    },
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(GreatHall, 54).exterior(),
                            RoomRequirement::new(Gallery, 34).exterior(),
                            RoomRequirement::new(Chapel, 20).exterior(),
                            RoomRequirement::new(Storage, 14),
                            RoomRequirement::new(StairHall, 18),
                        ],
                    },
                ],
                wall_style: WallStyle::TimberFrame,
                timber_frame_style: Some(TimberFrameStyle::EarlyModernOrnate),
                upper_storey_projection_metres: 0.24,
                roof_pitch_degrees: 54.0,
            },
            BuildingArchetype::CastleGatehouse => Self {
                archetype,
                seed,
                footprint: Footprint::Rectangle {
                    width: 10,
                    depth: 6,
                },
                storey_height_metres: 3.4,
                storeys: vec![
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(Passage, 18).exterior(),
                            RoomRequirement::new(Guardroom, 18)
                                .exterior()
                                .beside(Passage),
                            RoomRequirement::new(Armoury, 12).beside(Guardroom),
                            RoomRequirement::new(StairHall, 12),
                        ],
                    },
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(GreatHall, 24).exterior(),
                            RoomRequirement::new(Guardroom, 16).exterior(),
                            RoomRequirement::new(Armoury, 10),
                            RoomRequirement::new(StairHall, 10),
                        ],
                    },
                ],
                wall_style: WallStyle::Stone,
                timber_frame_style: None,
                upper_storey_projection_metres: 0.0,
                roof_pitch_degrees: 48.0,
            },
            BuildingArchetype::CourtyardCastle => Self {
                archetype,
                seed,
                footprint: Footprint::Courtyard {
                    width: 18,
                    depth: 16,
                    wing: 4,
                    gate_width: 4,
                },
                storey_height_metres: 3.5,
                storeys: vec![
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(Passage, 24).exterior(),
                            RoomRequirement::new(GreatHall, 55).exterior(),
                            RoomRequirement::new(Kitchen, 30).exterior(),
                            RoomRequirement::new(Guardroom, 35).exterior(),
                            RoomRequirement::new(Armoury, 24),
                            RoomRequirement::new(Storage, 35),
                            RoomRequirement::new(StairHall, 25),
                        ],
                    },
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(Gallery, 50).exterior(),
                            RoomRequirement::new(GreatHall, 55).exterior(),
                            RoomRequirement::new(Chapel, 28).exterior(),
                            RoomRequirement::new(Bedchamber, 34).exterior(),
                            RoomRequirement::new(Guardroom, 30).exterior(),
                            RoomRequirement::new(StairHall, 25),
                        ],
                    },
                ],
                wall_style: WallStyle::Stone,
                timber_frame_style: None,
                upper_storey_projection_metres: 0.0,
                roof_pitch_degrees: 52.0,
            },
        }
    }
}

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

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct RoundTower {
    pub centre: Vec2,
    pub radius_metres: f32,
    pub wall_height_metres: f32,
    pub wall_thickness_metres: f32,
    pub roof: Option<RoofPiece>,
    pub battlement: Option<BattlementKind>,
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
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Bartizan {
    pub centre: Vec2,
    pub base_height_metres: f32,
    pub radius_metres: f32,
    pub height_metres: f32,
    pub roofed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildingPlan {
    pub archetype: BuildingArchetype,
    pub seed: u64,
    pub footprint: Footprint,
    pub storey_height_metres: f32,
    pub wall_style: WallStyle,
    pub timber_frame_style: Option<TimberFrameStyle>,
    pub upper_storey_projection_metres: f32,
    pub storeys: Vec<StoreyPlan>,
    pub roofs: Vec<RoofPiece>,
    pub roof_dormers: Vec<RoofDormer>,
    pub towers: Vec<RoundTower>,
    pub stairs: Vec<Stair>,
    pub battlements: Vec<BattlementRun>,
    pub wall_walks: Vec<WallWalk>,
    pub bartizans: Vec<Bartizan>,
}

impl BuildingPlan {
    pub fn dimensions_metres(&self) -> Vec2 {
        let (width, depth) = self.footprint.dimensions();
        Vec2::new(
            f32::from(width) * CELL_SIZE_METRES,
            f32::from(depth) * CELL_SIZE_METRES,
        )
    }
}
