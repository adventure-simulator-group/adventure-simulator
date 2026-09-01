use super::*;

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
