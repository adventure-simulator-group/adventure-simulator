use super::*;

/// Stable grid address used by editor commands. Unlike resolved mesh IDs, this
/// remains meaningful when the building is regenerated after an edit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct WallSelector {
    pub storey_level: u16,
    pub cell: Cell,
    pub direction: Direction,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
    /// A finish attached to one semantic wall rather than the programme as a
    /// whole. Timber/plaster makes that wall's fachwerk part of the wall,
    /// instead of an independently editable collection of members.
    SetWallMaterial {
        wall: WallSelector,
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

/// The editable, pre-mesh portion of a building plan.  Both generated and
/// freeform buildings use these exact semantic storeys, wall segments,
/// openings, roof recipes, and finish overrides.  A player build is detached
/// from its programme, not converted into unrelated render primitives.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditableBuildingAssembly {
    pub footprint: Footprint,
    pub storey_height_metres: f32,
    pub wall_style: WallStyle,
    #[serde(default = "default_interior_wall_finish")]
    pub interior_wall_finish: InteriorWallFinish,
    #[serde(default)]
    pub wall_style_overrides: Vec<WallStyleOverride>,
    pub timber_frame_style: Option<TimberFrameStyle>,
    pub upper_storey_projection_metres: f32,
    pub storeys: Vec<StoreyPlan>,
    pub roofs: Vec<RoofPiece>,
    pub roof_dormers: Vec<RoofDormer>,
    pub stairs: Vec<Stair>,
}

impl EditableBuildingAssembly {
    pub fn empty() -> Self {
        Self {
            footprint: Footprint::Rectangle { width: 1, depth: 1 },
            storey_height_metres: 3.0,
            wall_style: WallStyle::TimberFrame,
            interior_wall_finish: InteriorWallFinish::Plastered,
            wall_style_overrides: Vec::new(),
            timber_frame_style: Some(TimberFrameStyle::LateMedieval),
            upper_storey_projection_metres: 0.0,
            storeys: Vec::new(),
            roofs: Vec::new(),
            roof_dormers: Vec::new(),
            stairs: Vec::new(),
        }
    }

    pub fn from_plan(plan: &BuildingPlan) -> Self {
        Self {
            footprint: plan.footprint,
            storey_height_metres: plan.storey_height_metres,
            wall_style: plan.wall_style,
            interior_wall_finish: InteriorWallFinish::Plastered,
            wall_style_overrides: plan.wall_style_overrides.clone(),
            timber_frame_style: plan.timber_frame_style,
            upper_storey_projection_metres: plan.upper_storey_projection_metres,
            storeys: plan.storeys.clone(),
            roofs: plan.roofs.clone(),
            roof_dormers: plan.roof_dormers.clone(),
            stairs: plan.stairs.clone(),
        }
    }

    pub fn wall_style_for(&self, selector: WallSelector) -> WallStyle {
        self.wall_style_overrides
            .iter()
            .rev()
            .find(|override_| override_.wall == selector)
            .map_or(self.wall_style, |override_| override_.style)
    }

    pub(super) fn storey_mut(&mut self, level: u16) -> &mut StoreyPlan {
        if let Some(index) = self.storeys.iter().position(|storey| storey.level == level) {
            return &mut self.storeys[index];
        }
        self.storeys.push(StoreyPlan {
            level,
            rooms: Vec::new(),
            walls: Vec::new(),
            openings: Vec::new(),
        });
        self.storeys.sort_by_key(|storey| storey.level);
        self.storeys
            .iter_mut()
            .find(|storey| storey.level == level)
            .expect("new storey must be present")
    }

    pub(super) fn has_wall(&self, selector: WallSelector) -> bool {
        self.storeys
            .iter()
            .find(|storey| storey.level == selector.storey_level)
            .is_some_and(|storey| {
                storey
                    .walls
                    .iter()
                    .any(|wall| wall.cell == selector.cell && wall.direction == selector.direction)
            })
    }
}

/// Freeform saves are disposable development data.  Version two deliberately
/// replaces the former generic-cuboid list with the shared building assembly.
pub const PLAYER_BUILD_DOCUMENT_SCHEMA_VERSION: u32 = 2;
