use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildingPlan {
    pub archetype: BuildingArchetype,
    pub seed: u64,
    pub footprint: Footprint,
    pub storey_height_metres: f32,
    pub wall_style: WallStyle,
    #[serde(default)]
    pub wall_style_overrides: Vec<WallStyleOverride>,
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
