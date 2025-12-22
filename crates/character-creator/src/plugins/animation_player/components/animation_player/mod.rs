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
                "WASD: rotate camera and zoom\n",
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
        mut cameras: Query<&mut crate::plugins::animation_player::components::OrbitalCamera>,
        time: Res<Time>,
    ) {
        let right_stick_x = gamepad.get(GamepadAxis::RightStickX).unwrap_or(0.0);
        let right_stick_y = gamepad.get(GamepadAxis::RightStickY).unwrap_or(0.0);

        let stick = Vec2::new(right_stick_x, right_stick_y);
        const DEADZONE_SQUARED: f32 = 0.01;
        if stick.length_squared() < DEADZONE_SQUARED {
            return;
        }

        for mut camera in &mut cameras {
            camera.yaw -= right_stick_x * 2.0 * time.delta_secs();
            camera.radius -= right_stick_y * 10.0 * time.delta_secs();
            camera.radius = camera.radius.clamp(2.0, 20.0);
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
        mut cameras: Query<&mut crate::plugins::animation_player::components::OrbitalCamera>,
        time: Res<Time>,
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

        for mut camera in &mut cameras {
            if keyboard_input.pressed(KeyCode::KeyA) {
                camera.yaw -= 2.0 * time.delta_secs();
            }
            if keyboard_input.pressed(KeyCode::KeyD) {
                camera.yaw += 2.0 * time.delta_secs();
            }

            if keyboard_input.pressed(KeyCode::KeyW) {
                camera.radius -= 10.0 * time.delta_secs();
            }
            if keyboard_input.pressed(KeyCode::KeyS) {
                camera.radius += 10.0 * time.delta_secs();
            }

            camera.radius = camera.radius.clamp(2.0, 20.0);
        }
    }
}
