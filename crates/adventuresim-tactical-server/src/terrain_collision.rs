use super::*;

pub(super) fn collider(
    terrain: &SceneTerrain,
    recipe: Option<&FaultScarpRecipe>,
) -> Result<Collider> {
    recipe.map_or_else(
        || Ok(terrain.collider()),
        |recipe| {
            fault_scarp_patch(terrain, *recipe)
                .map(|patch| patch.collider_with_terrain(terrain))
                .map_err(|reason| BevyError::from(reason.to_owned()))
        },
    )
}

pub(super) fn spawn_scene(
    commands: &mut Commands,
    scene_id: String,
    terrain: SceneTerrain,
    ground: SceneGround,
    environment: SceneEnvironment,
    terrain_patch: Option<&SceneTerrainPatch>,
    fault_scarp: Option<FaultScarpRecipe>,
) {
    let terrain_collider = terrain_patch.map_or_else(
        || terrain.collider(),
        |patch| patch.collider_with_terrain(&terrain),
    );
    let mut scene = commands.spawn((
        Replicated,
        SceneId(scene_id),
        terrain,
        ground,
        RigidBody::Static,
        CollisionLayers::new(TACTICAL_TERRAIN_LAYER, LayerMask::ALL),
        terrain_collider,
        Transform::default(),
    ));
    scene.insert(environment);
    if let Some(recipe) = fault_scarp {
        scene.insert(recipe);
    }
}
