#[derive(Resource)]
struct CaptureState {
    output: Option<PathBuf>,
    settle_frames: u32,
    settled: u32,
    primed: bool,
    in_flight: bool,
    manifest: CaptureManifest,
}

#[derive(Clone, Serialize)]
struct CaptureManifest {
    schema_version: u16,
    fixture: &'static str,
    view: &'static str,
    seed: u64,
    resolution: [u32; 2],
    room_count: usize,
    wall_count: usize,
    opening_count: usize,
    roof_piece_count: usize,
    roof_dormer_count: usize,
    roof_assembly_count: usize,
    roof_graph_hash: String,
    roof_face_ids: Vec<u64>,
    roof_edge_ids: Vec<u64>,
    roof_cut_ids: Vec<u64>,
    roof_support_node_ids: Vec<u64>,
    roof_drainage_terminal_ids: Vec<u64>,
    roof_drainage_network_ids: Vec<u64>,
    roof_drainage_channel_ids: Vec<u64>,
    roof_drainage_outlet_ids: Vec<u64>,
    roof_drainage_route_ids: Vec<u64>,
    roof_render_item_count: usize,
    roof_render_multiset_hash: String,
    rendered_roof_item_count: usize,
    rendered_roof_hash: String,
    tower_count: usize,
    square_tower_count: usize,
    curtain_wall_count: usize,
    stair_count: usize,
    battlement_run_count: usize,
    wall_walk_count: usize,
    defensive_circuit_count: usize,
    defensive_junction_count: usize,
    tower_portal_count: usize,
    gate_defense_count: usize,
    firing_position_count: usize,
    gate_closure_count: usize,
    resolved_solid_count: usize,
    resolved_void_count: usize,
    resolved_owner_count: usize,
    rendered_owner_count: usize,
    rendered_resolved_solid_count: usize,
    resolver_schema_version: u16,
    resolved_geometry_hash: String,
    resolved_solid_multiset_hash: String,
    rendered_geometry_hash: String,
    source_revision: String,
    source_dirty_fingerprint: String,
    plan_hash: String,
    evidence_hash: String,
    pixel_hash: String,
    focus_kind: Option<&'static str>,
    focused_tower_index: Option<usize>,
    focused_tower_indices: Vec<usize>,
    focused_wall_index: Option<usize>,
    focused_resolved_item_ids: Vec<u64>,
    focused_resolved_void_ids: Vec<u64>,
    focused_roof_item_ids: Vec<u64>,
    section_removed_roof_item_ids: Vec<u64>,
    visible_focused_roof_item_count: usize,
    focused_projected_ray_count: usize,
    projected_defense_kind: Option<&'static str>,
    projected_defense_deployment: Option<&'static str>,
    projected_tactical_target: Option<&'static str>,
    visible_focused_resolved_item_count: usize,
    focused_bounds_fraction: [f32; 4],
    camera_position: [f32; 3],
    camera_target: [f32; 3],
    required_focus_object_count: usize,
    visible_focus_object_count: usize,
    focus_requirements_met: bool,
    lighting_preset: &'static str,
    sun_direction: [f32; 3],
    sun_illuminance_lux: f32,
    ambient_brightness: f32,
    ambient_color: [f32; 3],
    lighting_calibration_bounds_fraction: [f32; 4],
    median_luminance_percent: u8,
    dark_clipped_bps: u16,
    bright_clipped_bps: u16,
    luminance_separation_percent: u8,
    shadow_luminance_percent: u8,
    plan_audit_issue_count: usize,
    audited_closed_mesh_count: usize,
    mesh_integrity_issue_count: usize,
    bartizan_count: usize,
    observed_mesh_count: usize,
    visible_mesh_count: usize,
    active_camera_count: usize,
    subject_pixel_bps: u16,
    validation_passed: bool,
    opening_profile: Option<&'static str>,
    wall_section_kind: Option<&'static str>,
    focused_assembly_owner_id: Option<u32>,
    focused_resolved_geometry_hash: Option<String>,
    section_cut_applied: bool,
    section_removed_item_ids: Vec<u64>,
    inside_label_visible: bool,
    outside_label_visible: bool,
    wall_thickness_metres: Option<f32>,
    scale_figure_height_metres: Option<f32>,
    scale_figure_visible: bool,
    section_annotation: String,
    section_annotation_visible: bool,
    exterior_throat_bounds_fraction: [f32; 4],
    interior_mouth_bounds_fraction: [f32; 4],
    church_program_hash: String,
    church_bay_labels: Vec<String>,
    church_support_node_ids: Vec<u64>,
    church_opening_ids: Vec<u64>,
    church_focused_roles: Vec<String>,
    church_target_component_ids: Vec<String>,
    church_target_item_ids: Vec<u64>,
    church_required_roles: Vec<String>,
    church_cut_plane: Option<[f32; 4]>,
    church_removed_target_item_ids: Vec<u64>,
    church_legend_visible: bool,
    timber_program_hash: String,
    timber_program: Option<String>,
    timber_assembly_id: Option<u64>,
    timber_member_ids: Vec<u64>,
    timber_joint_ids: Vec<u64>,
    timber_node_ids: Vec<u64>,
    timber_focused_roles: Vec<String>,
    timber_role_item_ids: std::collections::BTreeMap<String, Vec<u64>>,
    timber_role_bounds_fraction: std::collections::BTreeMap<String, [f32; 4]>,
    timber_target_component_ids: Vec<String>,
    timber_focus_interface_ids: Vec<u64>,
    timber_required_roles: Vec<String>,
    timber_cut_plane: Option<[f32; 4]>,
    timber_removed_target_item_ids: Vec<u64>,
    timber_legend_visible: bool,
    artillery_assembly_id: Option<u64>,
    artillery_phase: Option<String>,
    artillery_curtain_ids: Vec<u64>,
    artillery_rondel_ids: Vec<u64>,
    artillery_station_ids: Vec<u64>,
    artillery_route_surface_ids: Vec<u64>,
    artillery_fire_ray_count: usize,
    artillery_support_node_ids: Vec<u64>,
    artillery_ditch_void_id: Option<u64>,
    artillery_bridge_state: Option<String>,
    artillery_focused_roles: Vec<String>,
    artillery_role_item_ids: std::collections::BTreeMap<String, Vec<u64>>,
    artillery_role_bounds_fraction: std::collections::BTreeMap<String, [f32; 4]>,
    artillery_target_component_ids: Vec<String>,
    artillery_cut_plane: Option<[f32; 4]>,
    artillery_removed_target_item_ids: Vec<u64>,
    artillery_legend_visible: bool,
}

#[derive(Resource)]
struct RenderPalette {
    plaster: Handle<StandardMaterial>,
    brick: Handle<StandardMaterial>,
    stone: Handle<StandardMaterial>,
    earth: Handle<StandardMaterial>,
    timber: Handle<StandardMaterial>,
    roof: Handle<StandardMaterial>,
    roof_secondary: Handle<StandardMaterial>,
    floor: Handle<StandardMaterial>,
    cutaway: Handle<StandardMaterial>,
    door: Handle<StandardMaterial>,
    glass: Handle<StandardMaterial>,
    void: Handle<StandardMaterial>,
    stair: Handle<StandardMaterial>,
    room_floors: Vec<Handle<StandardMaterial>>,
}

#[derive(Component)]
struct ClosedSolid;

#[derive(Component)]
struct GeometryOwner(u32);

#[derive(Clone, Copy, Component)]
enum OpeningBoundaryKind {
    ExteriorThroat,
    InteriorMouth,
}

#[derive(Component)]
struct OpeningBoundary(OpeningBoundaryKind);

#[derive(Component)]
struct ResolvedRenderItem {
    id: u64,
    fingerprint: u64,
    local_half_size: Vec3,
}

/// Renderer correspondence for polygonal roof authority. Roof faces and
/// enclosure faces are not cuboidal S0 solids, so they use an independent
/// exact-ID/fingerprint multiset instead of contaminating the resolved-solid
/// correspondence contract.
#[derive(Component)]
struct RoofRenderItem {
    id: u64,
    fingerprint: u64,
    local_center: Vec3,
    local_half_size: Vec3,
}

#[derive(Component)]
struct LightingCalibration {
    local_center: Vec3,
    local_half_size: Vec3,
}

/// Render-only depth cue. Future collision/nav extraction must ignore entities
/// carrying this marker and consume the semantic shell/portal recipe instead.
#[derive(Component)]
struct NonCollidingVisualization;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum EditorTarget {
    Wall(WallSelector),
    Opening(WallSelector),
    TimberMember(u64),
}

#[derive(Component)]
struct EditorSelectable(EditorTarget);

#[derive(Component)]
struct EditorBuildingEntity;

#[derive(Component)]
struct EditorEnvironmentEntity;

#[derive(Component)]
struct PlayerBuildEntity;

/// World-space bounds of a detached floor piece. This makes the semantic
/// player-build renderer inspectable by circulation tests and automation
/// without reading back GPU mesh buffers.
#[derive(Clone, Copy, Component)]
#[allow(dead_code)]
struct PlayerBuildFloorPrism {
    min: Vec3,
    max: Vec3,
}

/// Marks the long local axis of an inspectable detached stair stringer.
#[derive(Component)]
struct PlayerBuildStairStringer;

/// Conservative world-space bounds for a player-build box primitive. These
/// are intentionally ECS data so route tests and external inspection tools
/// can validate the scene without GPU readback or screenshots.
#[derive(Clone, Copy, Component)]
#[allow(dead_code)]
struct PlayerBuildRenderPrism {
    min: Vec3,
    max: Vec3,
}

/// Scoped while the shared wall renderer is producing a detached/freeform
/// assembly. Keeping this at the primitive spawn boundary means every host
/// wall and fachwerk beam receives exactly the same visibility control.
#[derive(Clone, Copy, Resource)]
struct PlayerBuildSpawnContext {
    storey: usize,
    role: EditorVisibilityRole,
}

/// Render metadata used by the build-mode visibility controls. It is kept on
/// both generated programme geometry and freeform parts so the controls have
/// one authoritative ECS path.
#[derive(Clone, Copy, Component, Debug, Eq, PartialEq)]
struct EditorVisibilityTarget {
    storey: usize,
    role: EditorVisibilityRole,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditorVisibilityRole {
    Wall,
    Floor,
    Structure,
    Roof,
}

/// The opaque material assigned at scene setup. Ghost and cutaway states
/// replace the active handle transiently, then restore this exact handle.
#[derive(Component)]
struct EditorBaseMaterial(Handle<StandardMaterial>);

/// Avoid allocating a fresh translucent material every UI frame while a
/// visibility control remains selected.
#[derive(Component)]
struct EditorAppearanceIsTranslucent(bool);

#[derive(Component)]
struct EditorFachwerkForFinishedWall;
