use std::fmt;

use bevy::math::{IVec2, Vec2, Vec3};
use clap::ValueEnum;
use serde::{Deserialize, Deserializer, Serialize, de};

pub const CELL_SIZE_METRES: f32 = 1.5;
pub const WALL_THICKNESS_METRES: f32 = 0.18;
pub const GRID_UNITS_PER_CELL: i32 = 30;
pub const GRID_UNIT_METRES: f32 = CELL_SIZE_METRES / GRID_UNITS_PER_CELL as f32;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, Serialize, Deserialize)]
pub struct GridPoint {
    pub x: i32,
    pub z: i32,
}

impl GridPoint {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }

    pub fn metres(self) -> Vec2 {
        Vec2::new(
            self.x as f32 * GRID_UNIT_METRES,
            self.z as f32 * GRID_UNIT_METRES,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CellDiameter(u16);

impl CellDiameter {
    pub const fn new(cells: u16) -> Option<Self> {
        if cells == 0 { None } else { Some(Self(cells)) }
    }

    pub const fn cells(self) -> u16 {
        self.0
    }

    pub const fn grid_units(self) -> i32 {
        self.0 as i32 * GRID_UNITS_PER_CELL
    }

    pub fn metres(self) -> f32 {
        f32::from(self.0) * CELL_SIZE_METRES
    }

    pub const fn try_from_grid_units(units: i32) -> Option<Self> {
        if units <= 0 || units % GRID_UNITS_PER_CELL != 0 {
            None
        } else {
            let cells = units / GRID_UNITS_PER_CELL;
            if cells > u16::MAX as i32 {
                None
            } else {
                Some(Self(cells as u16))
            }
        }
    }
}

impl<'de> Deserialize<'de> for CellDiameter {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let cells = u16::deserialize(deserializer)?;
        Self::new(cells)
            .ok_or_else(|| de::Error::custom("tower diameter must contain at least one whole cell"))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct GridLength(i32);

impl GridLength {
    pub const fn new(units: i32) -> Option<Self> {
        if units > 0 { Some(Self(units)) } else { None }
    }

    pub const fn units(self) -> i32 {
        self.0
    }

    pub fn metres(self) -> f32 {
        self.0 as f32 * GRID_UNIT_METRES
    }
}

impl<'de> Deserialize<'de> for GridLength {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let units = i32::deserialize(deserializer)?;
        Self::new(units).ok_or_else(|| de::Error::custom("grid length must be positive"))
    }
}

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
    Nave,
    Chancel,
    Sacristy,
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
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
    FachwerkCottage,
    FachwerkMerchantHouse,
    RenaissanceTownHall,
    Cathedral,
    CastleGatehouse,
    CourtyardCastle,
    WalledKeep,
    ArtilleryRondelCastle,
}

/// Frozen project type for the first cathedral kernel.  The orientation and
/// bay counts are design inputs, not claims that every northern-German church
/// shared this arrangement.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChurchProgram {
    pub liturgical_east: Direction,
    pub nave_bays: u8,
    pub transept_bays: u8,
    pub choir_bays: u8,
    pub apse_sides: u8,
    pub aisles: u8,
    pub bay_length_cells: u8,
    pub nave_width_cells: u8,
    pub aisle_width_cells: u8,
    pub material: WallMaterialClass,
}

impl ChurchProgram {
    pub const URBAN_BRICK_BASILICA: Self = Self {
        liturgical_east: Direction::East,
        nave_bays: 4,
        transept_bays: 1,
        choir_bays: 2,
        apse_sides: 5,
        aisles: 3,
        bay_length_cells: 3,
        nave_width_cells: 4,
        aisle_width_cells: 2,
        material: WallMaterialClass::CathedralMasonry,
    };
}

impl BuildingArchetype {
    pub const ALL: [Self; 10] = [
        Self::TownHouse,
        Self::HallHouse,
        Self::FachwerkCottage,
        Self::FachwerkMerchantHouse,
        Self::RenaissanceTownHall,
        Self::Cathedral,
        Self::CastleGatehouse,
        Self::CourtyardCastle,
        Self::WalledKeep,
        Self::ArtilleryRondelCastle,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::TownHouse => "town-house",
            Self::HallHouse => "hall-house",
            Self::FachwerkCottage => "fachwerk-cottage",
            Self::FachwerkMerchantHouse => "fachwerk-merchant-house",
            Self::RenaissanceTownHall => "renaissance-town-hall",
            Self::Cathedral => "cathedral",
            Self::CastleGatehouse => "castle-gatehouse",
            Self::CourtyardCastle => "courtyard-castle",
            Self::WalledKeep => "walled-keep",
            Self::ArtilleryRondelCastle => "artillery-rondel-castle",
        }
    }
}

/// High-level input recipe for procedural building generation.
///
/// The recipe is intentionally allowed to describe combinations that cannot be
/// built. The public [`crate::generate`] boundary is the validator: every
/// successful result has passed the complete structural audit, while an
/// unbuildable recipe returns [`crate::GenerationError`].
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
    /// Optional explicit kernel demonstrator used by deterministic proof plans.
    /// Curated archetypes leave this unset.
    #[serde(default)]
    pub roof_demonstrator: Option<RoofKind>,
    /// Present only when a church-specific structural program, rather than
    /// the generic room allocator, is authoritative.
    #[serde(default)]
    pub church_program: Option<ChurchProgram>,
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
                roof_demonstrator: None,
                church_program: None,
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
                roof_demonstrator: None,
                church_program: None,
            },
            BuildingArchetype::FachwerkCottage => Self {
                archetype,
                seed,
                footprint: Footprint::Rectangle { width: 7, depth: 8 },
                storey_height_metres: 2.8,
                storeys: vec![StoreyProgram {
                    rooms: vec![
                        RoomRequirement::new(CommonRoom, 18)
                            .exterior()
                            .beside(Kitchen),
                        RoomRequirement::new(Kitchen, 10).exterior().beside(Pantry),
                        RoomRequirement::new(Pantry, 5),
                        RoomRequirement::new(Bedchamber, 12).exterior(),
                        RoomRequirement::new(EntranceHall, 7).exterior(),
                    ],
                }],
                wall_style: WallStyle::TimberFrame,
                timber_frame_style: Some(TimberFrameStyle::NorthernCloseStudded),
                upper_storey_projection_metres: 0.0,
                roof_pitch_degrees: 53.0,
                roof_demonstrator: None,
                church_program: None,
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
                roof_demonstrator: None,
                church_program: None,
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
                roof_demonstrator: None,
                church_program: None,
            },
            BuildingArchetype::Cathedral => Self {
                archetype,
                seed,
                footprint: Footprint::Rectangle {
                    width: 28,
                    depth: 14,
                },
                storey_height_metres: 5.8,
                storeys: vec![StoreyProgram {
                    rooms: vec![
                        RoomRequirement::new(Nave, 190).exterior().beside(Chancel),
                        RoomRequirement::new(Chancel, 70).exterior().beside(Nave),
                        RoomRequirement::new(Chapel, 32).exterior(),
                        RoomRequirement::new(Sacristy, 24)
                            .exterior()
                            .beside(Chancel),
                        RoomRequirement::new(EntranceHall, 20).exterior(),
                    ],
                }],
                wall_style: WallStyle::Stone,
                timber_frame_style: None,
                upper_storey_projection_metres: 0.0,
                roof_pitch_degrees: 58.0,
                roof_demonstrator: None,
                church_program: Some(ChurchProgram::URBAN_BRICK_BASILICA),
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
                roof_demonstrator: None,
                church_program: None,
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
                roof_demonstrator: None,
                church_program: None,
            },
            BuildingArchetype::WalledKeep => Self {
                archetype,
                seed,
                footprint: Footprint::Rectangle { width: 9, depth: 8 },
                storey_height_metres: 3.4,
                storeys: vec![
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(EntranceHall, 14).exterior(),
                            RoomRequirement::new(Guardroom, 18).exterior(),
                            RoomRequirement::new(Armoury, 12),
                            RoomRequirement::new(Storage, 18),
                            RoomRequirement::new(StairHall, 10),
                        ],
                    },
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(GreatHall, 28).exterior(),
                            RoomRequirement::new(Kitchen, 12).exterior(),
                            RoomRequirement::new(Guardroom, 12).exterior(),
                            RoomRequirement::new(StairHall, 10),
                            RoomRequirement::new(Storage, 10),
                        ],
                    },
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(Bedchamber, 20).exterior(),
                            RoomRequirement::new(Guardroom, 16).exterior(),
                            RoomRequirement::new(Armoury, 12),
                            RoomRequirement::new(StairHall, 10),
                            RoomRequirement::new(Storage, 14),
                        ],
                    },
                ],
                wall_style: WallStyle::Stone,
                timber_frame_style: None,
                upper_storey_projection_metres: 0.0,
                roof_pitch_degrees: 0.0,
                roof_demonstrator: None,
                church_program: None,
            },
            BuildingArchetype::ArtilleryRondelCastle => Self {
                archetype,
                seed,
                // The room-grid footprint is the retained older keep. The
                // independent ArtilleryCastleAssembly owns the much larger
                // 36 x 30 m clear court and retrofit enceinte around it.
                footprint: Footprint::Rectangle { width: 9, depth: 8 },
                storey_height_metres: 3.4,
                storeys: vec![
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(EntranceHall, 14).exterior(),
                            RoomRequirement::new(Guardroom, 18).exterior(),
                            RoomRequirement::new(Armoury, 12),
                            RoomRequirement::new(Storage, 18),
                            RoomRequirement::new(StairHall, 10),
                        ],
                    },
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(GreatHall, 28).exterior(),
                            RoomRequirement::new(Kitchen, 12).exterior(),
                            RoomRequirement::new(Guardroom, 12).exterior(),
                            RoomRequirement::new(StairHall, 10),
                            RoomRequirement::new(Storage, 10),
                        ],
                    },
                    StoreyProgram {
                        rooms: vec![
                            RoomRequirement::new(Bedchamber, 20).exterior(),
                            RoomRequirement::new(Guardroom, 16).exterior(),
                            RoomRequirement::new(Armoury, 12),
                            RoomRequirement::new(StairHall, 10),
                            RoomRequirement::new(Storage, 14),
                        ],
                    },
                ],
                wall_style: WallStyle::Stone,
                timber_frame_style: None,
                upper_storey_projection_metres: 0.0,
                roof_pitch_degrees: 0.0,
                roof_demonstrator: None,
                church_program: None,
            },
        }
    }
}

pub const BUILDING_DOCUMENT_SCHEMA_VERSION: u32 = 2;

/// Stable grid address used by editor commands. Unlike resolved mesh IDs, this
/// remains meaningful when the building is regenerated after an edit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct WallSelector {
    pub storey_level: u16,
    pub cell: Cell,
    pub direction: Direction,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum BuildingEdit {
    AddOpening {
        wall: WallSelector,
        opening_kind: OpeningKind,
        width_metres: f32,
        sill_metres: f32,
        height_metres: f32,
    },
    RemoveOpening {
        wall: WallSelector,
    },
    SetWallStyle {
        style: WallStyle,
    },
    SetTimberFrameStyle {
        style: TimberFrameStyle,
    },
}

/// Versioned, serializable authority edited by the interactive building
/// editor. Resolved geometry is deliberately absent: it is regenerated and
/// audited transactionally from this document.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildingDocument {
    pub schema_version: u32,
    pub program: BuildingProgram,
    #[serde(default)]
    pub edits: Vec<BuildingEdit>,
}

impl BuildingDocument {
    pub fn fixture(archetype: BuildingArchetype, seed: u64) -> Self {
        Self {
            schema_version: BUILDING_DOCUMENT_SCHEMA_VERSION,
            program: BuildingProgram::fixture(archetype, seed),
            edits: Vec::new(),
        }
    }
}

/// A freeform player-build document is intentionally separate from the
/// generated-programme document.  Parts may overlap, be unsupported, or fail
/// to describe a recognisable historic programme; renderability and lossless
/// saving are the only commit requirements.
pub const PLAYER_BUILD_DOCUMENT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerBuildPartKind {
    Wall,
    Room,
    Door,
    Gate,
    Window,
    ArrowSlit,
    Roof,
    Stair,
    SiteObject,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerBuildMaterial {
    Stone,
    Brick,
    Plaster,
    TimberFrame,
    Timber,
    Tile,
    Thatch,
    Earth,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerBuildPart {
    pub id: u64,
    pub kind: PlayerBuildPartKind,
    pub material: PlayerBuildMaterial,
    pub storey: u16,
    pub x_metres: f32,
    pub z_metres: f32,
    pub elevation_metres: f32,
    pub rotation_degrees: f32,
    pub width_metres: f32,
    pub depth_metres: f32,
    pub height_metres: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum PlayerBuildEdit {
    Place {
        part: PlayerBuildPart,
    },
    Move {
        id: u64,
        x_metres: f32,
        z_metres: f32,
    },
    Resize {
        id: u64,
        width_metres: f32,
        depth_metres: f32,
        height_metres: f32,
    },
    Rotate {
        id: u64,
        rotation_degrees: f32,
    },
    Remove {
        id: u64,
    },
    SetMaterial {
        id: u64,
        material: PlayerBuildMaterial,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerBuildDocument {
    pub schema_version: u32,
    #[serde(default)]
    pub parts: Vec<PlayerBuildPart>,
}

impl PlayerBuildDocument {
    pub fn empty() -> Self {
        Self {
            schema_version: PLAYER_BUILD_DOCUMENT_SCHEMA_VERSION,
            parts: Vec::new(),
        }
    }

    /// Applies a freeform edit without consulting the strict programme audit.
    /// This preserves deliberate player experiments while still rejecting data
    /// that no renderer can safely represent.
    pub fn apply(&self, edit: PlayerBuildEdit) -> Result<Self, String> {
        if self.schema_version != PLAYER_BUILD_DOCUMENT_SCHEMA_VERSION {
            return Err(format!(
                "player-build document schema {} is unsupported; expected {}",
                self.schema_version, PLAYER_BUILD_DOCUMENT_SCHEMA_VERSION
            ));
        }
        let mut next = self.clone();
        match edit {
            PlayerBuildEdit::Place { part } => {
                if !part_dimensions_are_renderable(&part) {
                    return Err(
                        "player-build part dimensions must be finite and positive".to_owned()
                    );
                }
                if next.parts.iter().any(|existing| existing.id == part.id) {
                    return Err(format!("player-build part {} already exists", part.id));
                }
                next.parts.push(part);
                next.parts.sort_by_key(|part| part.id);
            }
            PlayerBuildEdit::Move {
                id,
                x_metres,
                z_metres,
            } => {
                let part = next.part_mut(id)?;
                if !x_metres.is_finite() || !z_metres.is_finite() {
                    return Err("player-build position must be finite".to_owned());
                }
                part.x_metres = x_metres;
                part.z_metres = z_metres;
            }
            PlayerBuildEdit::Resize {
                id,
                width_metres,
                depth_metres,
                height_metres,
            } => {
                let part = next.part_mut(id)?;
                part.width_metres = width_metres;
                part.depth_metres = depth_metres;
                part.height_metres = height_metres;
                if !part_dimensions_are_renderable(part) {
                    return Err(
                        "player-build part dimensions must be finite and positive".to_owned()
                    );
                }
            }
            PlayerBuildEdit::Rotate {
                id,
                rotation_degrees,
            } => {
                if !rotation_degrees.is_finite() {
                    return Err("player-build rotation must be finite".to_owned());
                }
                next.part_mut(id)?.rotation_degrees = rotation_degrees.rem_euclid(360.0);
            }
            PlayerBuildEdit::Remove { id } => {
                let count = next.parts.len();
                next.parts.retain(|part| part.id != id);
                if next.parts.len() == count {
                    return Err(format!("player-build part {id} was not found"));
                }
            }
            PlayerBuildEdit::SetMaterial { id, material } => next.part_mut(id)?.material = material,
        }
        Ok(next)
    }

    fn part_mut(&mut self, id: u64) -> Result<&mut PlayerBuildPart, String> {
        self.parts
            .iter_mut()
            .find(|part| part.id == id)
            .ok_or_else(|| format!("player-build part {id} was not found"))
    }
}

fn part_dimensions_are_renderable(part: &PlayerBuildPart) -> bool {
    [
        part.x_metres,
        part.z_metres,
        part.elevation_metres,
        part.rotation_degrees,
        part.width_metres,
        part.depth_metres,
        part.height_metres,
    ]
    .into_iter()
    .all(f32::is_finite)
        && part.width_metres > 0.0
        && part.depth_metres > 0.0
        && part.height_metres > 0.0
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StructuralNode {
    pub id: StructuralNodeId,
    pub owner: GeometryOwnerId,
    pub kind: StructuralNodeKind,
    pub position: Vec3,
    pub supported_by: Vec<StructuralNodeId>,
    pub grounded: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SupportInterface {
    pub id: ResolvedItemId,
    pub owner: GeometryOwnerId,
    pub node: StructuralNodeId,
    pub bounds: ResolvedBounds,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct DrainageRoute {
    pub id: ResolvedItemId,
    pub owner: GeometryOwnerId,
    pub outlet_void: ResolvedItemId,
    pub inlet: Vec3,
    pub outlet: Vec3,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrainageCatchment {
    pub id: ResolvedItemId,
    pub owner: GeometryOwnerId,
    pub walk_solid: ResolvedItemId,
    pub toe_channel_solids: Vec<ResolvedItemId>,
    pub drainage_surface: ResolvedItemId,
    pub outlet_route: ResolvedItemId,
    pub centre: Vec3,
    /// Canonical local +X direction in plan.
    pub tangent: Vec2,
    /// Physical downhill direction in plan.
    pub outward: Vec2,
    pub length_metres: f32,
    pub width_metres: f32,
    pub inner_elevation_metres: f32,
    pub outer_elevation_metres: f32,
    /// Signed local-X coordinate of the exact scupper inlet at the channel end.
    pub outlet_along_metres: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct RoofDrainageSample {
    /// Point sampled on the authoritative weather face.
    pub surface_point: Vec3,
    /// First physical contact with the receiving eave/valley channel.
    pub channel_inlet: Vec3,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoofDrainageNetwork {
    pub id: ResolvedItemId,
    pub owner: GeometryOwnerId,
    pub face: ResolvedItemId,
    pub catchment: ResolvedItemId,
    pub receiving_edge: ResolvedItemId,
    pub samples: Vec<RoofDrainageSample>,
    pub channel_floor: ResolvedItemId,
    pub channel_lips: [ResolvedItemId; 2],
    /// Physical perimeter collector segments connecting this catchment gutter
    /// to its shared outlet station.
    pub collector_solids: Vec<ResolvedItemId>,
    pub outlet_station: ResolvedItemId,
    pub outlet_void: ResolvedItemId,
    pub downspout: Option<ResolvedItemId>,
    pub channel_high: Vec3,
    pub channel_low: Vec3,
    pub discharge: Vec3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoofDrainageDisposition {
    BoundDownspout,
    FreeDripToParentRoof,
    FreeDripToGround,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum RoofDrainageRecipient {
    GroundSplashApron,
    ParentRoofFace {
        roof: RoofAssemblyId,
        face: ResolvedItemId,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoofDrainageOutletStation {
    pub id: ResolvedItemId,
    pub owner: GeometryOwnerId,
    pub disposition: RoofDrainageDisposition,
    pub member_networks: Vec<ResolvedItemId>,
    pub host_wall: Option<WallAssemblyId>,
    pub facade_contact: Option<Vec3>,
    pub outlet_void: ResolvedItemId,
    pub downspout: Option<ResolvedItemId>,
    pub recipient: RoofDrainageRecipient,
    pub recipient_surface: ResolvedItemId,
    pub discharge: Vec3,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct DefenderSample {
    pub owner: GeometryOwnerId,
    pub stance: Vec3,
    pub eye: Vec3,
    pub target: Vec3,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct JunctionBond {
    pub id: ResolvedItemId,
    pub owners: [GeometryOwnerId; 2],
    pub bounds: ResolvedBounds,
    pub minimum_interface_area_square_metres: f32,
    pub maximum_penetration_metres: f32,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectedDefenseKind {
    Machicolation,
    Breteche,
    Hoarding,
    Bartizan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectedDefenseMaterial {
    Masonry,
    Timber,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectedDefensePhase {
    PermanentMainWork,
    TemporaryCampaignWork,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectedDefenseDeployment {
    Permanent,
    SocketsOnly,
    Deployed,
}

/// Tactical reason for installing a projected defense. These labels keep
/// curated full-building fixtures from becoming an ahistorical catalogue of
/// unrelated devices merely because the resolver can construct them.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectedDefenseTarget {
    GateApproach,
    ThreatenedWallFoot,
    ThreatenedCorner,
    CampaignSiegeFront,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ProjectedDefensePath {
    Linear {
        start: Vec2,
        end: Vec2,
        outward: Direction,
    },
    Round {
        centre: Vec2,
        radius_metres: f32,
        outward: Direction,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ProjectedDefenseRay {
    pub owner: GeometryOwnerId,
    pub throat: ResolvedItemId,
    pub stance: Vec3,
    pub origin: Vec3,
    pub target: Vec3,
    pub range: ProjectedDefenseRange,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectedDefenseRange {
    Near,
    Middle,
    Far,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ProjectedDefenseWorkingPoint {
    pub owner: GeometryOwnerId,
    pub aperture: ResolvedItemId,
    pub stance: Vec3,
    pub eye: Vec3,
    pub support_solid: ResolvedItemId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectedDefenseHostTopology {
    LinearFace,
    CornerFaces,
    Buttress,
}

/// Exact source-wall identity replaced by the resolved host masonry.
///
/// The renderer suppresses these legacy wall cells and draws the resolved,
/// opening-aware replacement instead. This prevents a projected defense from
/// manufacturing an additive witness wall unrelated to the building model.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectedDefenseHostWallSource {
    pub storey_level: u16,
    pub wall_index: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectedDefenseAssembly {
    pub owner: GeometryOwnerId,
    /// Authoritative resolved masonry host. It is deliberately a distinct
    /// owner so junction bonds and void subtraction cannot be satisfied by
    /// projection-owned witness geometry.
    pub host_owner: GeometryOwnerId,
    pub host_wall_solids: Vec<ResolvedItemId>,
    pub host_buttress_solids: Vec<ResolvedItemId>,
    pub host_source_walls: Vec<ProjectedDefenseHostWallSource>,
    pub host_top_elevation_metres: f32,
    pub host_topology: ProjectedDefenseHostTopology,
    pub host_walk_solid: ResolvedItemId,
    pub host_portal_void: Option<ResolvedItemId>,
    pub host_bond: Option<ResolvedItemId>,
    pub beam_socket_voids: Vec<ResolvedItemId>,
    pub socket_joists: Vec<(ResolvedItemId, ResolvedItemId)>,
    pub kind: ProjectedDefenseKind,
    pub material: ProjectedDefenseMaterial,
    pub phase: ProjectedDefensePhase,
    pub deployment: ProjectedDefenseDeployment,
    pub tactical_target: ProjectedDefenseTarget,
    pub path: ProjectedDefensePath,
    pub floor_elevation_metres: f32,
    pub clear_width_metres: f32,
    pub clear_height_metres: f32,
    pub projection_metres: f32,
    pub breastwork_height_metres: f32,
    pub roofed: bool,
    pub floor_solids: Vec<ResolvedItemId>,
    pub throat_voids: Vec<ResolvedItemId>,
    pub access_portal: Option<ResolvedItemId>,
    pub access_landing: Option<ResolvedItemId>,
    pub firing_apertures: Vec<ResolvedItemId>,
    pub support_nodes: Vec<StructuralNodeId>,
    pub drain_route: Option<ResolvedItemId>,
    pub drainage_catchments: Vec<ResolvedItemId>,
    /// Roof or exposed coping catchments, distinct from the occupied floor.
    pub weather_catchments: Vec<ResolvedItemId>,
    pub weathering_solids: Vec<ResolvedItemId>,
    /// Physical enclosure walls/posts and wall plates carrying a roof. Empty
    /// for unroofed work; authoritative rather than proof-only geometry.
    pub roof_support_solids: Vec<ResolvedItemId>,
    /// Roof-bearing node whose parents are the independently supported inner
    /// and outer plate lines.
    pub roof_bearing_node: Option<StructuralNodeId>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ResolvedGeometry {
    pub schema_version: u16,
    pub solids: Vec<ResolvedSolid>,
    pub surfaces: Vec<ResolvedSurface>,
    pub voids: Vec<ResolvedVoid>,
    pub structural_nodes: Vec<StructuralNode>,
    pub support_interfaces: Vec<SupportInterface>,
    pub drainage_routes: Vec<DrainageRoute>,
    pub drainage_catchments: Vec<DrainageCatchment>,
    pub roof_drainage_networks: Vec<RoofDrainageNetwork>,
    pub roof_drainage_outlets: Vec<RoofDrainageOutletStation>,
    pub defender_samples: Vec<DefenderSample>,
    pub junction_bonds: Vec<JunctionBond>,
    pub projected_defense_rays: Vec<ProjectedDefenseRay>,
    pub projected_defense_working_points: Vec<ProjectedDefenseWorkingPoint>,
}

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

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ChurchAssemblyId(pub u64);

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ChurchDatum {
    pub floor_metres: f32,
    pub aisle_eave_metres: f32,
    pub clerestory_sill_metres: f32,
    pub nave_eave_metres: f32,
    pub vault_crown_metres: f32,
    pub bell_floor_metres: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChurchBayAssembly {
    pub axis_index: u8,
    pub axis_metres: f32,
    pub range: ChurchRange,
    pub pier_nodes: [StructuralNodeId; 2],
    pub pier_solids: [ResolvedItemId; 2],
    pub arcade_solids: [ResolvedItemId; 2],
    /// West/east pier bearings for each south/north arcade span.
    pub arcade_bearing_nodes: [[StructuralNodeId; 2]; 2],
    /// Positive contact regions at the two ends of each arcade span.
    pub arcade_bearing_interfaces: [[ResolvedItemId; 2]; 2],
    pub buttress_nodes: [StructuralNodeId; 2],
    pub buttress_solids: [ResolvedItemId; 2],
    pub clerestory_openings: [OpeningAssemblyId; 2],
    pub vault_solids: Vec<ResolvedItemId>,
    /// Transverse springing/tie members that carry vault thrust from the
    /// arcade pier line to the exterior buttress line at both bay ends.
    pub vault_thrust_solids: Vec<ResolvedItemId>,
    pub vault_load_surfaces: Vec<ResolvedItemId>,
    /// South/north vault springings whose parents include both bay-end piers
    /// and both corresponding exterior buttresses.
    pub vault_spring_nodes: Vec<StructuralNodeId>,
    pub vault_bearing_interfaces: Vec<ResolvedItemId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChurchCrossingAssembly {
    pub bounds: ResolvedBounds,
    pub pier_nodes: [StructuralNodeId; 4],
    pub pier_solids: [ResolvedItemId; 4],
    pub arch_solids: [ResolvedItemId; 4],
    pub arch_bearing_nodes: [[StructuralNodeId; 2]; 4],
    pub arch_bearing_interfaces: [[ResolvedItemId; 2]; 4],
    pub vault_solids: Vec<ResolvedItemId>,
    pub buttress_nodes: [StructuralNodeId; 4],
    pub buttress_solids: [ResolvedItemId; 4],
    pub vault_thrust_solids: Vec<ResolvedItemId>,
    pub vault_load_surfaces: Vec<ResolvedItemId>,
    pub vault_spring_nodes: Vec<StructuralNodeId>,
    pub vault_bearing_interfaces: Vec<ResolvedItemId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChurchChoirAssembly {
    pub bay_axes_metres: Vec<f32>,
    pub pier_nodes: Vec<StructuralNodeId>,
    pub pier_solids: Vec<ResolvedItemId>,
    pub buttress_nodes: Vec<StructuralNodeId>,
    pub buttress_solids: Vec<ResolvedItemId>,
    pub arch_solids: Vec<ResolvedItemId>,
    pub arch_bearing_nodes: Vec<[StructuralNodeId; 2]>,
    pub arch_bearing_interfaces: Vec<[ResolvedItemId; 2]>,
    pub apse_facets: Vec<WallAssemblyId>,
    pub radial_buttress_nodes: Vec<StructuralNodeId>,
    pub radial_buttress_solids: Vec<ResolvedItemId>,
    pub floor_solids: Vec<ResolvedItemId>,
    pub vault_solids: Vec<ResolvedItemId>,
    pub vault_thrust_solids: Vec<ResolvedItemId>,
    pub vault_load_surfaces: Vec<ResolvedItemId>,
    pub vault_spring_nodes: Vec<StructuralNodeId>,
    pub vault_bearing_interfaces: Vec<ResolvedItemId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChurchTowerAssembly {
    pub centre: Vec2,
    pub footprint_size_metres: Vec2,
    pub wall_ids: Vec<WallAssemblyId>,
    pub west_portal: OpeningAssemblyId,
    pub nave_passage: OpeningAssemblyId,
    /// Public approach slab on the protected centreline immediately outside
    /// the west portal.  Together with `vestibule_surface` and
    /// `nave_entry_surface` this is the authoritative ground-level route,
    /// rather than a semantic opening label attached to a nave-wide surface.
    pub exterior_approach_surface: ResolvedItemId,
    /// Tower-floor patch between the two opposed doorway reveals.  Bell
    /// service branches from this exact shared node.
    pub vestibule_surface: ResolvedItemId,
    /// Nave-side arrival patch immediately beyond the tower/nave passage.
    pub nave_entry_surface: ResolvedItemId,
    pub stair_index: usize,
    pub stair_bearing_node: StructuralNodeId,
    pub stair_newel_solid: ResolvedItemId,
    pub stair_tread_solids: Vec<ResolvedItemId>,
    pub stair_tread_interfaces: Vec<ResolvedItemId>,
    pub landing_solids: Vec<ResolvedItemId>,
    pub guard_solids: Vec<ResolvedItemId>,
    /// Four bearing slabs surrounding the authoritative stairwell opening.
    pub bell_floor_solids: Vec<ResolvedItemId>,
    /// Four corner route patches on the bearing ring.  These prevent the
    /// traversal graph from cutting diagonally across the stairwell void.
    pub bell_floor_corner_surfaces: Vec<ResolvedItemId>,
    pub bell_frame_solids: Vec<ResolvedItemId>,
    pub bell_solid: ResolvedItemId,
    pub bell_openings: Vec<OpeningAssemblyId>,
    /// Fixed service ladder from the bell floor to the roof stage.
    pub roof_ladder_solids: Vec<ResolvedItemId>,
    pub roof_service_surface: ResolvedItemId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChurchRouteKind {
    PublicProcessional,
    TowerService,
    BellService,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ChurchRouteEdge {
    pub from: ResolvedItemId,
    pub to: ResolvedItemId,
    pub clear_width_metres: f32,
    pub clear_headroom_metres: f32,
    pub through_opening: Option<OpeningAssemblyId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChurchCirculationRoute {
    pub kind: ChurchRouteKind,
    pub waypoints: Vec<Vec3>,
    pub width_metres: f32,
    pub headroom_metres: f32,
    pub surface_ids: Vec<ResolvedItemId>,
    /// Authoritative walkable/climbable solids (spiral treads, landings,
    /// bearing-ring floor pieces, and ladder rungs) used by route adjacency.
    pub traversable_solid_ids: Vec<ResolvedItemId>,
    pub edges: Vec<ChurchRouteEdge>,
    pub opening_ids: Vec<OpeningAssemblyId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChurchAssembly {
    pub id: ChurchAssemblyId,
    pub program: ChurchProgram,
    pub datum: ChurchDatum,
    pub west_elevation_metres: f32,
    pub nave_axes_metres: Vec<f32>,
    pub crossing_axis_metres: f32,
    pub choir_axes_metres: Vec<f32>,
    pub bay_assemblies: Vec<ChurchBayAssembly>,
    pub crossing: ChurchCrossingAssembly,
    pub choir: ChurchChoirAssembly,
    pub tower: ChurchTowerAssembly,
    pub circulation: Vec<ChurchCirculationRoute>,
    pub floor_solids: Vec<ResolvedItemId>,
    pub roof_assemblies: Vec<RoofAssemblyId>,
}

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
    pub wall_assemblies: Vec<WallAssembly>,
    pub opening_assemblies: Vec<OpeningAssembly>,
    pub roofs: Vec<RoofPiece>,
    pub roof_dormers: Vec<RoofDormer>,
    pub roof_assemblies: Vec<RoofAssembly>,
    pub towers: Vec<RoundTower>,
    pub square_towers: Vec<SquareTower>,
    pub stairs: Vec<Stair>,
    pub battlements: Vec<BattlementRun>,
    pub crowns: Vec<CrownAssembly>,
    pub projected_defenses: Vec<ProjectedDefenseAssembly>,
    pub resolved_geometry: ResolvedGeometry,
    pub wall_walks: Vec<WallWalk>,
    pub defensive_junctions: Vec<DefensiveJunction>,
    pub defensive_circuits: Vec<DefensiveCircuit>,
    pub tower_portals: Vec<TowerPortal>,
    pub curtain_walls: Vec<CurtainWallRun>,
    pub gate_defenses: Vec<GateDefense>,
    pub gatehouse_assemblies: Vec<GatehouseAssemblySpec>,
    pub bartizans: Vec<Bartizan>,
    pub church: Option<ChurchAssembly>,
    pub timber_frame: Option<TimberFrameAssembly>,
    pub castle_phase: Option<CastleConstructionPhase>,
    pub artillery_castle: Option<ArtilleryCastleAssembly>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn wall(id: u64) -> PlayerBuildPart {
        PlayerBuildPart {
            id,
            kind: PlayerBuildPartKind::Wall,
            material: PlayerBuildMaterial::Stone,
            storey: 0,
            x_metres: 0.0,
            z_metres: 0.0,
            elevation_metres: 0.0,
            rotation_degrees: 0.0,
            width_metres: 3.0,
            depth_metres: WALL_THICKNESS_METRES,
            height_metres: 3.0,
        }
    }

    #[test]
    fn player_build_edits_preserve_non_programme_geometry() {
        let placed = PlayerBuildDocument::empty()
            .apply(PlayerBuildEdit::Place { part: wall(7) })
            .unwrap()
            .apply(PlayerBuildEdit::Move {
                id: 7,
                x_metres: 7.25,
                z_metres: -1.5,
            })
            .unwrap()
            .apply(PlayerBuildEdit::Rotate {
                id: 7,
                rotation_degrees: -90.0,
            })
            .unwrap();
        assert_eq!(placed.parts[0].x_metres, 7.25);
        assert_eq!(placed.parts[0].rotation_degrees, 270.0);
        let decoded: PlayerBuildDocument = serde_json::from_slice(
            &serde_json::to_vec(&placed).expect("player build should serialize"),
        )
        .expect("player build should deserialize");
        assert_eq!(decoded.parts, placed.parts);
    }

    #[test]
    fn player_build_rejects_only_unrenderable_part_data() {
        let invalid = PlayerBuildDocument::empty().apply(PlayerBuildEdit::Place {
            part: PlayerBuildPart {
                width_metres: 0.0,
                ..wall(1)
            },
        });
        assert!(invalid.is_err());
    }
}
