use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::prelude::SceneVistaBundle;
use bevy::prelude::*;

pub(crate) fn vista_bundle(input: &TacticalSceneInput) -> Option<SceneVistaBundle> {
    Some(SceneVistaBundle {
        scene_digest: input.digest().expect("loaded scene input was validated"),
        playable_half_extent_metres: Vec2::new(
            f32::from(input.playable.width.saturating_sub(1)) * input.playable.spacing_metres * 0.5,
            f32::from(input.playable.depth.saturating_sub(1)) * input.playable.spacing_metres * 0.5,
        ),
        distant_buildings: input.distant_buildings.clone(),
        streets: input.streets.clone(),
        yards: input.yards.clone(),
        lods: input.vista.lods.clone(),
    })
}

pub(crate) fn spawn_world_bounds(commands: &mut Commands, width: f32, depth: f32) {
    commands.spawn((
        RigidBody::Static,
        CollisionLayers::new(TACTICAL_TERRAIN_LAYER, LayerMask::ALL),
        Transform::default(),
        children![
            (
                Collider::half_space(Vec3::X),
                Transform::from_xyz(-width * 0.5, 0.0, 0.0)
            ),
            (
                Collider::half_space(Vec3::NEG_X),
                Transform::from_xyz(width * 0.5, 0.0, 0.0)
            ),
            (
                Collider::half_space(Vec3::Z),
                Transform::from_xyz(0.0, 0.0, -depth * 0.5)
            ),
            (
                Collider::half_space(Vec3::NEG_Z),
                Transform::from_xyz(0.0, 0.0, depth * 0.5)
            )
        ],
    ));
}
