use bevy::prelude::*;

use std::time::Duration;

use crate::plugins::animation_player::resources::{Animations, SceneHandle};

pub struct AnimationPlayer;

#[derive(Component, Copy, Clone)]
pub struct CharacterBaseRotation(pub Quat);

impl AnimationPlayer {
    // An `AnimationPlayer` is automatically added to the scene when it's ready.
    // When the player is added, start the animation.
    pub fn start(
        mut commands: Commands,
        animations: Res<Animations>,
        mut players: Query<
            (Entity, &Transform, &mut bevy::animation::AnimationPlayer),
            Added<bevy::animation::AnimationPlayer>,
        >,
        asset_server: Res<AssetServer>,
        scene_handle: Option<Res<SceneHandle>>,
    ) {
        for (entity, transform, mut player) in &mut players {
            if let Some(scene_handle) = &scene_handle {
                let load_state = asset_server.get_load_state(&scene_handle.scene);
                info!(
                    "AnimationPlayer ready on entity {:?}, scene load state: {:?}",
                    entity, load_state
                );
            } else {
                warn!(
                    "AnimationPlayer ready on entity {:?} but scene handle not found",
                    entity
                );
            }

            let mut transitions = AnimationTransitions::new();

            // Make sure to start the animation via the `AnimationTransitions`
            // component. The `AnimationTransitions` component wants to manage all
            // the animations and will get confused if the animations are started
            // directly via the `AnimationPlayer`.
            transitions
                .play(&mut player, animations.animations[0], Duration::ZERO)
                .repeat();

            commands
                .entity(entity)
                .insert(AnimationGraphHandle(animations.graph_handle.clone()))
                .insert(transitions)
                .insert(CharacterBaseRotation(transform.rotation));
        }
    }
}
