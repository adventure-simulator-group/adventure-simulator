use bevy::prelude::*;

use std::time::Duration;

use bevy::animation::RepeatAnimation;

use crate::plugins::animation_player::resources::{Animations, SceneHandle};

pub struct AnimationPlayer;

#[derive(Component, Copy, Clone)]
pub struct CharacterBaseRotation(pub Quat);

impl AnimationPlayer {
    pub fn spawn(mut commands: Commands) {
        // Instructions
        commands.spawn((
            Text::new(concat!(
                "space: play / pause\n",
                "up / down: playback speed\n",
                "left / right: seek\n",
                "1-3: play N times\n",
                "L: loop forever\n",
                "return: change animation\n",
                "gamepad: rotate character with left stick\n",
            )),
            Node {
                position_type: PositionType::Absolute,
                top: px(12),
                left: px(12),
                ..default()
            },
        ));
    }

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

    pub fn gamepad_control(
        gamepad: Single<&Gamepad>,
        mut characters: Query<(&mut Transform, &CharacterBaseRotation), With<AnimationGraphHandle>>,
    ) {
        let Some(left_stick_x) = gamepad.get(GamepadAxis::LeftStickX) else {
            return;
        };
        let Some(left_stick_y) = gamepad.get(GamepadAxis::LeftStickY) else {
            return;
        };

        let stick = Vec2::new(left_stick_x, left_stick_y);
        const DEADZONE_SQUARED: f32 = 0.01;
        if stick.length_squared() < DEADZONE_SQUARED {
            return;
        }

        let direction = Vec3::new(stick.x, 0.0, -stick.y).normalize();
        let yaw = direction.x.atan2(direction.z);
        let target_rotation = Quat::from_rotation_y(yaw);

        for (mut transform, base_rotation) in &mut characters {
            transform.rotation = target_rotation * base_rotation.0;
        }
    }

    pub fn keyboard_control(
        keyboard_input: Res<ButtonInput<KeyCode>>,
        mut animation_players: Query<(
            &mut bevy::animation::AnimationPlayer,
            &mut AnimationTransitions,
        )>,
        animations: Res<Animations>,
        mut current_animation: Local<usize>,
    ) {
        for (mut player, mut transitions) in &mut animation_players {
            let Some((&playing_animation_index, _)) = player.playing_animations().next() else {
                continue;
            };

            if keyboard_input.just_pressed(KeyCode::Space) {
                let playing_animation = player.animation_mut(playing_animation_index).unwrap();
                if playing_animation.is_paused() {
                    playing_animation.resume();
                } else {
                    playing_animation.pause();
                }
            }

            if keyboard_input.just_pressed(KeyCode::ArrowUp) {
                let playing_animation = player.animation_mut(playing_animation_index).unwrap();
                let speed = playing_animation.speed();
                playing_animation.set_speed(speed * 1.2);
            }

            if keyboard_input.just_pressed(KeyCode::ArrowDown) {
                let playing_animation = player.animation_mut(playing_animation_index).unwrap();
                let speed = playing_animation.speed();
                playing_animation.set_speed(speed * 0.8);
            }

            if keyboard_input.just_pressed(KeyCode::ArrowLeft) {
                let playing_animation = player.animation_mut(playing_animation_index).unwrap();
                let elapsed = playing_animation.seek_time();
                playing_animation.seek_to(elapsed - 0.1);
            }

            if keyboard_input.just_pressed(KeyCode::ArrowRight) {
                let playing_animation = player.animation_mut(playing_animation_index).unwrap();
                let elapsed = playing_animation.seek_time();
                playing_animation.seek_to(elapsed + 0.1);
            }

            if keyboard_input.just_pressed(KeyCode::Enter) {
                *current_animation = (*current_animation + 1) % animations.animations.len();

                transitions
                    .play(
                        &mut player,
                        animations.animations[*current_animation],
                        Duration::from_millis(250),
                    )
                    .repeat();
            }

            if keyboard_input.just_pressed(KeyCode::Digit1) {
                let playing_animation = player.animation_mut(playing_animation_index).unwrap();
                playing_animation
                    .set_repeat(RepeatAnimation::Count(1))
                    .replay();
            }

            if keyboard_input.just_pressed(KeyCode::Digit2) {
                let playing_animation = player.animation_mut(playing_animation_index).unwrap();
                playing_animation
                    .set_repeat(RepeatAnimation::Count(2))
                    .replay();
            }

            if keyboard_input.just_pressed(KeyCode::Digit3) {
                let playing_animation = player.animation_mut(playing_animation_index).unwrap();
                playing_animation
                    .set_repeat(RepeatAnimation::Count(3))
                    .replay();
            }

            if keyboard_input.just_pressed(KeyCode::KeyL) {
                let playing_animation = player.animation_mut(playing_animation_index).unwrap();
                playing_animation.set_repeat(RepeatAnimation::Forever);
            }
        }
    }
}
