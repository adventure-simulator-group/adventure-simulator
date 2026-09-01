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

mod artillery;
mod building_program;
mod church;
mod defenses;
mod editor;
mod layout;
mod plan;
mod player_build;
mod programme;
mod resolved_networks;
mod resolved_shapes;
mod timber;
mod walls;

pub use artillery::*;
pub use building_program::*;
pub use church::*;
pub use defenses::*;
pub use editor::*;
pub use layout::*;
pub use plan::*;
pub use player_build::*;
pub use programme::*;
pub use resolved_networks::*;
pub use resolved_shapes::*;
pub use timber::*;
pub use walls::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_build_edits_preserve_shared_wall_assembly() {
        let placed = PlayerBuildDocument::empty()
            .apply(PlayerBuildEdit::DrawWall {
                start: GridPoint::new(0, 0),
                end: GridPoint::new(3, 0),
                storey: 0,
                style: WallStyle::TimberFrame,
            })
            .unwrap()
            .apply(PlayerBuildEdit::SetWallMaterial {
                wall: WallSelector {
                    storey_level: 0,
                    cell: Cell::new(1, -1),
                    direction: Direction::North,
                },
                style: WallStyle::Brick,
            })
            .unwrap();
        assert_eq!(placed.assembly.storeys[0].walls.len(), 3);
        assert_eq!(
            placed.assembly.wall_style_for(WallSelector {
                storey_level: 0,
                cell: Cell::new(1, -1),
                direction: Direction::North,
            }),
            WallStyle::Brick
        );
        let decoded: PlayerBuildDocument = serde_json::from_slice(
            &serde_json::to_vec(&placed).expect("player build should serialize"),
        )
        .expect("player build should deserialize");
        assert_eq!(decoded.assembly.storeys[0].walls.len(), 3);
    }

    #[test]
    fn player_build_rejects_non_grid_wall_data() {
        let invalid = PlayerBuildDocument::empty().apply(PlayerBuildEdit::DrawWall {
            start: GridPoint::new(0, 0),
            end: GridPoint::new(1, 1),
            storey: 0,
            style: WallStyle::Stone,
        });
        assert!(invalid.is_err());
    }

    #[test]
    fn player_build_floor_tiles_are_semantic_room_cells() {
        let document = PlayerBuildDocument::empty()
            .apply(PlayerBuildEdit::PlaceFloorTile {
                cell: Cell::new(2, 3),
                storey: 0,
            })
            .unwrap();
        assert_eq!(
            document.assembly.storeys[0].rooms[0].cells,
            vec![Cell::new(2, 3)]
        );
        assert!(
            document
                .apply(PlayerBuildEdit::PlaceFloorTile {
                    cell: Cell::new(2, 3),
                    storey: 0,
                })
                .is_err()
        );
    }

    #[test]
    fn player_build_roof_edits_replace_one_semantic_roof_recipe() {
        let mut document = PlayerBuildDocument::empty();
        document.assembly.roofs.push(RoofPiece {
            kind: RoofKind::Gable,
            centre: Vec2::ZERO,
            size: Vec2::splat(3.0),
            base_height_metres: 3.0,
            pitch_degrees: 45.0,
            ridge_axis: RidgeAxis::X,
            eave_metres: 0.2,
            gable_profile: GableProfile::Plain,
        });
        let edited = document
            .apply(PlayerBuildEdit::UpdateRoof {
                index: 0,
                roof: RoofPiece {
                    pitch_degrees: 30.0,
                    kind: RoofKind::Hip,
                    ..document.assembly.roofs[0]
                },
            })
            .unwrap();
        assert_eq!(edited.assembly.roofs[0].kind, RoofKind::Hip);
        assert_eq!(edited.assembly.roofs[0].pitch_degrees, 30.0);
    }

    #[test]
    fn player_build_keeps_interior_finish_separate_from_exterior_wall_style() {
        let document = PlayerBuildDocument::empty()
            .apply(PlayerBuildEdit::SetInteriorWallFinish {
                finish: InteriorWallFinish::Boarded,
            })
            .unwrap();
        assert_eq!(document.assembly.wall_style, WallStyle::TimberFrame);
        assert_eq!(
            document.assembly.interior_wall_finish,
            InteriorWallFinish::Boarded
        );
    }

    #[test]
    fn player_build_analysis_is_advisory() {
        let document = PlayerBuildDocument::empty()
            .apply(PlayerBuildEdit::DrawWall {
                start: GridPoint::new(0, 0),
                end: GridPoint::new(1, 0),
                storey: 1,
                style: WallStyle::Stone,
            })
            .unwrap();
        assert_eq!(
            analyse_player_build(&document),
            vec![
                PlayerBuildAdvice::NoExteriorDoor,
                PlayerBuildAdvice::UpperStoreyWithoutSupport { storey: 1 },
            ]
        );
        assert_eq!(document.assembly.storeys[0].walls.len(), 1);
    }
}
