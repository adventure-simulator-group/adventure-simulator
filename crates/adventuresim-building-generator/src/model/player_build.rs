use super::*;

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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum PlayerBuildEdit {
    SetInteriorWallFinish {
        finish: InteriorWallFinish,
    },
    UpdateRoof {
        index: usize,
        roof: RoofPiece,
    },
    PlaceFloorTile {
        cell: Cell,
        storey: u16,
    },
    RemoveFloorTile {
        cell: Cell,
        storey: u16,
    },
    DrawWall {
        start: GridPoint,
        end: GridPoint,
        storey: u16,
        style: WallStyle,
    },
    RemoveWall {
        wall: WallSelector,
    },
    SetWallMaterial {
        wall: WallSelector,
        style: WallStyle,
    },
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
    pub assembly: EditableBuildingAssembly,
    /// Compatibility-only working data for callers compiled against the first
    /// editor ABI. It is never serialized and new edits reject it; all saved
    /// player buildings are semantic assemblies.
    #[serde(skip)]
    pub parts: Vec<PlayerBuildPart>,
}

impl PlayerBuildDocument {
    pub fn empty() -> Self {
        Self {
            schema_version: PLAYER_BUILD_DOCUMENT_SCHEMA_VERSION,
            assembly: EditableBuildingAssembly::empty(),
            parts: Vec::new(),
        }
    }

    pub fn from_plan(plan: &BuildingPlan) -> Self {
        Self {
            schema_version: PLAYER_BUILD_DOCUMENT_SCHEMA_VERSION,
            assembly: EditableBuildingAssembly::from_plan(plan),
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
            PlayerBuildEdit::SetInteriorWallFinish { finish } => {
                next.assembly.interior_wall_finish = finish;
            }
            PlayerBuildEdit::UpdateRoof { index, roof } => {
                if !roof_recipe_is_renderable(roof) {
                    return Err("roof dimensions, elevation, pitch, and eaves must be finite and positive where required".to_owned());
                }
                let Some(existing) = next.assembly.roofs.get_mut(index) else {
                    return Err(format!("roof piece {index} was not found"));
                };
                *existing = roof;
            }
            PlayerBuildEdit::PlaceFloorTile { cell, storey } => {
                let storey_plan = next.assembly.storey_mut(storey);
                if storey_plan
                    .rooms
                    .iter()
                    .any(|room| room.cells.contains(&cell))
                {
                    return Err("that floor tile already exists".to_owned());
                }
                let room = storey_plan.rooms.iter_mut().find(|room| room.id == 0);
                if let Some(room) = room {
                    room.cells.push(cell);
                    room.cells.sort();
                } else {
                    storey_plan.rooms.push(Room {
                        id: 0,
                        kind: RoomKind::CommonRoom,
                        cells: vec![cell],
                    });
                }
            }
            PlayerBuildEdit::RemoveFloorTile { cell, storey } => {
                let Some(storey_plan) = next
                    .assembly
                    .storeys
                    .iter_mut()
                    .find(|candidate| candidate.level == storey)
                else {
                    return Err("floor tile was not found".to_owned());
                };
                let mut removed = false;
                for room in &mut storey_plan.rooms {
                    let count = room.cells.len();
                    room.cells.retain(|candidate| *candidate != cell);
                    removed |= room.cells.len() != count;
                }
                storey_plan.rooms.retain(|room| !room.cells.is_empty());
                if !removed {
                    return Err("floor tile was not found".to_owned());
                }
            }
            PlayerBuildEdit::DrawWall {
                start,
                end,
                storey,
                style,
            } => {
                if start.x != end.x && start.z != end.z {
                    return Err("freeform walls must follow the square grid".to_owned());
                }
                if start == end {
                    return Err("freeform wall must span at least one grid cell".to_owned());
                }
                let mut added = 0;
                let mut overrides = Vec::new();
                {
                    let storey_plan = next.assembly.storey_mut(storey);
                    if start.z == end.z {
                        let direction = Direction::North;
                        for x in start.x.min(end.x)..start.x.max(end.x) {
                            let selector = WallSelector {
                                storey_level: storey,
                                cell: Cell::new(x as i16, (start.z - 1) as i16),
                                direction,
                            };
                            if !storey_plan.walls.iter().any(|wall| {
                                wall.cell == selector.cell && wall.direction == selector.direction
                            }) {
                                storey_plan.walls.push(WallSegment {
                                    cell: selector.cell,
                                    direction,
                                    inside_room: 0,
                                    outside_room: None,
                                });
                                overrides.push(WallStyleOverride {
                                    wall: selector,
                                    style,
                                });
                                added += 1;
                            }
                        }
                    } else {
                        let direction = Direction::East;
                        for z in start.z.min(end.z)..start.z.max(end.z) {
                            let selector = WallSelector {
                                storey_level: storey,
                                cell: Cell::new((start.x - 1) as i16, z as i16),
                                direction,
                            };
                            if !storey_plan.walls.iter().any(|wall| {
                                wall.cell == selector.cell && wall.direction == selector.direction
                            }) {
                                storey_plan.walls.push(WallSegment {
                                    cell: selector.cell,
                                    direction,
                                    inside_room: 0,
                                    outside_room: None,
                                });
                                overrides.push(WallStyleOverride {
                                    wall: selector,
                                    style,
                                });
                                added += 1;
                            }
                        }
                    }
                }
                next.assembly.wall_style_overrides.extend(overrides);
                if added == 0 {
                    return Err("the drawn wall already exists".to_owned());
                }
            }
            PlayerBuildEdit::RemoveWall { wall } => {
                let Some(storey) = next
                    .assembly
                    .storeys
                    .iter_mut()
                    .find(|storey| storey.level == wall.storey_level)
                else {
                    return Err("player-build wall was not found".to_owned());
                };
                let count = storey.walls.len();
                storey.walls.retain(|candidate| {
                    !(candidate.cell == wall.cell && candidate.direction == wall.direction)
                });
                if storey.walls.len() == count {
                    return Err("player-build wall was not found".to_owned());
                }
                next.assembly
                    .wall_style_overrides
                    .retain(|override_| override_.wall != wall);
            }
            PlayerBuildEdit::SetWallMaterial { wall, style } => {
                if !next.assembly.has_wall(wall) {
                    return Err("player-build wall was not found".to_owned());
                }
                next.assembly
                    .wall_style_overrides
                    .retain(|override_| override_.wall != wall);
                next.assembly
                    .wall_style_overrides
                    .push(WallStyleOverride { wall, style });
            }
            PlayerBuildEdit::Place { part: _ } => {
                return Err(
                    "generic freeform parts were replaced by semantic building assemblies"
                        .to_owned(),
                );
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

pub(super) const fn default_interior_wall_finish() -> InteriorWallFinish {
    InteriorWallFinish::Plastered
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlayerBuildAdvice {
    NoExteriorDoor,
    UpperStoreyWithoutSupport { storey: u16 },
}

/// Advisory-only findings for freeform construction.  These never change a
/// player-build document and therefore cannot turn historical or structural
/// preference into a hidden placement veto.
pub fn analyse_player_build(document: &PlayerBuildDocument) -> Vec<PlayerBuildAdvice> {
    let mut advice = Vec::new();
    let has_exterior_door = document
        .assembly
        .storeys
        .iter()
        .find(|storey| storey.level == 0)
        .is_some_and(|storey| {
            storey.openings.iter().any(|opening| {
                matches!(opening.kind, OpeningKind::Door | OpeningKind::Gate)
                    && storey
                        .walls
                        .get(opening.wall)
                        .is_some_and(|wall| wall.exterior())
            })
        });
    if !has_exterior_door {
        advice.push(PlayerBuildAdvice::NoExteriorDoor);
    }
    for storey in document
        .assembly
        .storeys
        .iter()
        .map(|storey| storey.level)
        .filter(|storey| *storey > 0)
        .collect::<std::collections::BTreeSet<_>>()
    {
        let has_lower_structure = document
            .assembly
            .storeys
            .iter()
            .any(|candidate| candidate.level < storey && !candidate.walls.is_empty());
        if !has_lower_structure {
            advice.push(PlayerBuildAdvice::UpperStoreyWithoutSupport { storey });
        }
    }
    advice
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

fn roof_recipe_is_renderable(roof: RoofPiece) -> bool {
    [
        roof.centre.x,
        roof.centre.y,
        roof.size.x,
        roof.size.y,
        roof.base_height_metres,
        roof.pitch_degrees,
        roof.eave_metres,
    ]
    .into_iter()
    .all(f32::is_finite)
        && roof.size.x > 0.0
        && roof.size.y > 0.0
        && roof.base_height_metres >= 0.0
        && roof.eave_metres >= 0.0
}
