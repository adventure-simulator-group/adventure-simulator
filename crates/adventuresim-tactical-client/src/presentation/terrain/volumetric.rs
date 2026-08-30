use super::*;

#[expect(
    clippy::too_many_arguments,
    reason = "presentation state is independently borrowed"
)]
pub(super) fn refresh_presentation(
    terrain: &SceneTerrain,
    environment: &SceneEnvironment,
    ground: Option<&SceneGround>,
    procedural_assets: &ProceduralEnvironmentAssets,
    graphics: &TacticalGraphicsSettings,
    vista: &ActiveVistaSurface,
    transition_collar: Option<TerrainTransitionCollar>,
    handle: &MeshMaterial3d<TacticalTerrainMaterial>,
    detail_handle: &MeshMaterial3d<TacticalTerrainMaterial>,
    patch: &mut TerrainDetailPatch,
    mesh_handle: &Mesh3d,
    triangle_count: &mut TerrainTriangleCount,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<TacticalTerrainMaterial>,
    images: &mut Assets<Image>,
) -> bool {
    if materials.get(&handle.0).is_none() || materials.get(&detail_handle.0).is_none() {
        return false;
    }
    let material = terrain_material(
        terrain,
        environment,
        ground,
        procedural_assets,
        images,
        &graphics.config.grass,
    );
    *materials
        .get_mut(&handle.0)
        .expect("checked terrain material") = material.clone();
    let mut detail_material = material;
    detail_material.base.depth_bias = DETAIL_PATCH_DEPTH_BIAS;
    detail_material.extension.detail_patch.x = 0.0;
    *materials
        .get_mut(&detail_handle.0)
        .expect("checked detail terrain material") = detail_material;
    if let Some(mut mesh) = meshes.get_mut(&mesh_handle.0) {
        let replacement =
            terrain_detail_patch_mesh(terrain, environment, vista, patch.centre, transition_collar);
        triangle_count.0 = mesh_triangle_count(&replacement);
        *mesh = replacement;
        patch.vista_revision = vista.revision();
    }
    true
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy presentation resources are explicit"
)]
pub(super) fn spawn_base_and_fault(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<TacticalTerrainMaterial>,
    entity: Entity,
    id: &SceneId,
    terrain: &SceneTerrain,
    material: TacticalTerrainMaterial,
    fault_scarp: Option<&FaultScarpRecipe>,
    transition_collar: Option<TerrainTransitionCollar>,
) {
    let fault_patch = fault_scarp.and_then(|recipe| fault_scarp_patch(terrain, *recipe).ok());
    let fault_material = fault_patch.as_ref().map(|_| {
        let mut fault_material = material.clone();
        // The patch owns its zero-offset rim, so it uses refined-terrain
        // presentation instead of the coarse heightfield discard path.
        fault_material.extension.detail_patch.x = 0.0;
        fault_material.base.depth_bias = 1.0;
        materials.add(fault_material)
    });
    let material = materials.add(material);
    let playable_mesh = transition_collar.map_or_else(
        || terrain.coarse_mesh(),
        |collar| terrain.coarse_mesh_with_transition(collar),
    );
    let triangle_count = mesh_triangle_count(&playable_mesh);
    commands.spawn((
        Name::new(format!("{} terrain mesh", id.0)),
        ScenePresentationOf(entity),
        TerrainMaterialPresentation,
        TerrainTriangleCount(triangle_count),
        Mesh3d(meshes.add(playable_mesh)),
        MeshMaterial3d(material),
    ));
    if let Some(patch) = fault_patch {
        let triangle_count = patch.triangle_count();
        commands.spawn((
            Name::new(format!("{} fault scarp mesh", id.0)),
            ScenePresentationOf(entity),
            TerrainTriangleCount(triangle_count),
            Mesh3d(meshes.add(terrain_patch_mesh(patch, terrain))),
            MeshMaterial3d(fault_material.expect("fault patch has a material")),
        ));
    }
}

fn terrain_patch_mesh(patch: SceneTerrainPatch, terrain: &SceneTerrain) -> Mesh {
    let uvs = patch
        .positions
        .iter()
        .map(|position| {
            [
                position[0] / terrain.width() + 0.5,
                position[2] / terrain.depth() + 0.5,
            ]
        })
        .collect::<Vec<_>>();
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, patch.positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, patch.normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(patch.indices));
    mesh
}

pub(super) fn append_detail_cell(
    indices: &mut Vec<u32>,
    positions: &[[f32; 3]],
    width: usize,
    index: u32,
    centre: Vec2,
    collar: Option<TerrainTransitionCollar>,
) {
    let width = width as u32;
    for triangle in [
        [index, index + width, index + 1],
        [index + 1, index + width, index + width + 1],
    ] {
        let triangle_centre = triangle
            .iter()
            .map(|&index| {
                let position = positions[index as usize];
                Vec2::new(position[0], position[2])
            })
            .sum::<Vec2>()
            / 3.0;
        if triangle_centre.distance(centre) <= DETAIL_PATCH_RADIUS_METRES
            && !collar.is_some_and(|collar| collar.cuts_out(triangle_centre))
        {
            indices.extend_from_slice(&triangle);
        }
    }
}
