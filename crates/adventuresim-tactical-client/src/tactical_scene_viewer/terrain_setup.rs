use super::*;

pub(super) fn spawn(
    commands: &mut Commands,
    input: &TacticalSceneInput,
    environment: SceneEnvironment,
    ground: SceneGround,
    terrain: SceneTerrain,
    terrain_patch: Option<&SceneTerrainPatch>,
) {
    let collider = terrain_patch.map_or_else(
        || terrain.collider(),
        |patch| patch.collider_with_terrain(&terrain),
    );
    let mut terrain_entity = commands.spawn((
        Name::new("Captured tactical terrain"),
        SceneId(input.scene_key.clone()),
        environment,
        ground,
        RigidBody::Static,
        CollisionLayers::new(TACTICAL_TERRAIN_LAYER, LayerMask::ALL),
        collider,
        terrain,
        Transform::default(),
    ));
    if let Some(fault_scarp) = input.fault_scarp {
        terrain_entity.insert(fault_scarp);
    }
}
