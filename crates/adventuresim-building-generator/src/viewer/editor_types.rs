#[derive(Clone, Copy, Eq, PartialEq)]
enum SceneSetup {
    Full,
    EditorInitial,
    EditorBuilding,
}

/// The visible editor modes deliberately follow the direct-manipulation
/// vocabulary used by the build workbench.  Only tools backed by the current
/// semantic document are enabled; unavailable modes remain discoverable
/// instead of pretending that a click has changed the building.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum EditorMode {
    Select,
    Construct,
    Openings,
    Roof,
    Site,
    Finish,
}

impl EditorMode {
    const ALL: [(Self, &'static str, &'static str); 6] = [
        (Self::Select, "Select", "1"),
        (Self::Construct, "Construct", "2"),
        (Self::Openings, "Openings", "3"),
        (Self::Roof, "Roof", "4"),
        (Self::Site, "Site", "5"),
        (Self::Finish, "Finish", "6"),
    ];

    fn availability(self) -> &'static str {
        match self {
            Self::Select => "Inspect walls, openings, and timber members directly on the building.",
            Self::Openings => "Select a wall, then place the audited window opening below.",
            Self::Finish => "Apply a compatible finish to the current programme.",
            Self::Construct => {
                "Freeform wall and room authoring requires the player-build document."
            }
            Self::Roof => "Freeform roof and stair handles require the player-build document.",
            Self::Site => "Site dressing requires the player-build document and site authority.",
        }
    }

    fn is_available(self) -> bool {
        matches!(
            self,
            Self::Select | Self::Construct | Self::Openings | Self::Finish
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum WallVisibility {
    Up,
    Cutaway,
    Down,
}

impl WallVisibility {
    fn next(self) -> Self {
        match self {
            Self::Up => Self::Cutaway,
            Self::Cutaway => Self::Down,
            Self::Down => Self::Up,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Up => "Walls: Up",
            Self::Cutaway => "Walls: Cutaway",
            Self::Down => "Walls: Down",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum RoofVisibility {
    Show,
    Ghost,
    Hide,
}

#[derive(Resource)]
struct EditorRuntime {
    document: BuildingDocument,
    plan: BuildingPlan,
    document_path: PathBuf,
    player_build: Option<PlayerBuildDocument>,
    player_build_path: Option<PathBuf>,
    selected_player_part: Option<u64>,
    selected_player_roof: Option<usize>,
    player_roof: RoofPiece,
    player_x_metres: f32,
    player_z_metres: f32,
    player_elevation_metres: f32,
    player_width_metres: f32,
    player_depth_metres: f32,
    player_height_metres: f32,
    player_rotation_degrees: f32,
    player_tool: PlayerBuildTool,
    player_material: PlayerBuildMaterial,
    wall_drag: Option<WallDrag>,
    wall_preview: Option<WallPreview>,
    pending_player_rebuild: bool,
    undo: Vec<BuildingDocument>,
    redo: Vec<BuildingDocument>,
    selected: Option<EditorTarget>,
    hovered: Option<EditorTarget>,
    error: Option<String>,
    status: String,
    window_width_metres: f32,
    window_sill_metres: f32,
    window_height_metres: f32,
    opening_kind: OpeningKind,
    mode: EditorMode,
    active_storey: usize,
    wall_visibility: WallVisibility,
    roof_visibility: RoofVisibility,
    show_generated_building: bool,
    pending_rebuild: bool,
}

impl EditorRuntime {
    fn new(
        document: BuildingDocument,
        plan: BuildingPlan,
        document_path: PathBuf,
        player_build: Option<PlayerBuildDocument>,
        player_build_path: Option<PathBuf>,
    ) -> Self {
        Self {
            document,
            plan,
            document_path,
            player_build,
            player_build_path,
            selected_player_part: None,
            selected_player_roof: None,
            player_roof: RoofPiece {
                kind: RoofKind::Gable,
                centre: Vec2::ZERO,
                size: Vec2::splat(3.0),
                base_height_metres: 3.0,
                pitch_degrees: 45.0,
                ridge_axis: RidgeAxis::X,
                eave_metres: 0.2,
                gable_profile: GableProfile::Plain,
            },
            player_x_metres: 0.0,
            player_z_metres: 0.0,
            player_elevation_metres: 0.0,
            player_width_metres: 3.0,
            player_depth_metres: WALL_THICKNESS_METRES,
            player_height_metres: 3.0,
            player_rotation_degrees: 0.0,
            player_tool: PlayerBuildTool::Wall,
            player_material: PlayerBuildMaterial::Stone,
            wall_drag: None,
            wall_preview: None,
            pending_player_rebuild: false,
            undo: Vec::new(),
            redo: Vec::new(),
            selected: None,
            hovered: None,
            error: None,
            status: "Ready".to_owned(),
            window_width_metres: 0.8,
            window_sill_metres: 0.9,
            window_height_metres: 1.1,
            opening_kind: OpeningKind::Window,
            mode: EditorMode::Select,
            active_storey: 0,
            wall_visibility: WallVisibility::Up,
            roof_visibility: RoofVisibility::Show,
            show_generated_building: true,
            pending_rebuild: false,
        }
    }

    fn highest_visible_storey(&self) -> usize {
        let highest_wall_storey = self
            .player_build
            .as_ref()
            .map(|document| {
                document
                    .assembly
                    .storeys
                    .iter()
                    .map(|storey| usize::from(storey.level))
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or_else(|| self.plan.storeys.len().saturating_sub(1));
        let has_roof = self
            .player_build
            .as_ref()
            .map_or(!self.plan.roofs.is_empty(), |document| {
                !document.assembly.roofs.is_empty()
            });
        highest_wall_storey + usize::from(has_roof)
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PlayerBuildTool {
    Wall,
    FloorTile,
}

#[derive(Clone, Copy)]
struct WallDrag {
    start: Vec2,
    camera: Entity,
}

#[derive(Clone, Copy)]
struct WallPreview {
    start: Vec2,
    end: Vec2,
}

/// Stable, UI-independent command ABI for editor tests, automation, and
/// future remote tooling.  UI interactions translate to these commands rather
/// than retaining a separate test-only behavior path.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action")]
pub(crate) enum EditorCommand {
    PlaceFloorTile {
        x_metres: f32,
        z_metres: f32,
        storey: u16,
    },
    DrawWall {
        start_x_metres: f32,
        start_z_metres: f32,
        end_x_metres: f32,
        end_z_metres: f32,
        material: PlayerBuildMaterial,
        storey: u16,
    },
    PlacePart {
        part: PlayerBuildPart,
    },
    MovePart {
        id: u64,
        x_metres: f32,
        z_metres: f32,
    },
    ResizePart {
        id: u64,
        width_metres: f32,
        depth_metres: f32,
        height_metres: f32,
    },
    RotatePart {
        id: u64,
        rotation_degrees: f32,
    },
    RemovePart {
        id: u64,
    },
    SetActiveStorey {
        storey: usize,
    },
    CycleWalls,
    CycleRoofs,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct EditorSnapshot {
    pub active_storey: usize,
    pub mode: EditorMode,
    pub walls: WallVisibility,
    pub roof: RoofVisibility,
    pub selected_part: Option<u64>,
    pub parts: Vec<PlayerBuildPart>,
    pub advice: Vec<String>,
    pub status: String,
    pub error: Option<String>,
}
