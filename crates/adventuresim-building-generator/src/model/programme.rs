use super::*;

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

/// A required, generated connection between occupied storeys.
///
/// This is programme intent rather than editable geometry.  The procedural
/// layout solver must satisfy it before walls and openings become authoritative;
/// detached player builds remain free to contain incomplete or blocked stairs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum VerticalConnectionRequirement {
    StraightStair {
        lowest_storey: u16,
        highest_storey: u16,
        landing_room: RoomKind,
    },
    TowerSpiral {
        lowest_storey: u16,
        highest_storey: u16,
    },
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

/// Room-side treatment of a timber-framed wall.  This is deliberately
/// independent from the weather face: visible fachwerk is an exterior
/// expression, while inhabited rooms were normally plastered.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteriorWallFinish {
    Plastered,
    Boarded,
    ExposedFrame,
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
