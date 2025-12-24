use bevy::prelude::*;

#[derive(Component)]
pub struct OrbitalCamera {
    pub radius: f32,
    pub pitch: f32,
    pub yaw: f32,
    pub focus: Option<Entity>,
}

impl Default for OrbitalCamera {
    fn default() -> Self {
        Self {
            radius: 5.0,
            pitch: 0.5,
            yaw: 0.0,
            focus: None,
        }
    }
}

impl OrbitalCamera {
    pub fn update(
        mut cameras: Query<(&mut Transform, &OrbitalCamera)>,
        transforms: Query<&GlobalTransform>,
    ) {
        for (mut camera_transform, orbit) in &mut cameras {
            if let Some(focus_entity) = orbit.focus {
                if let Ok(focus_transform) = transforms.get(focus_entity) {
                    let focus_translation = focus_transform.translation();

                    let rotation = Quat::from_axis_angle(Vec3::Y, orbit.yaw)
                        * Quat::from_axis_angle(Vec3::X, -orbit.pitch);

                    let offset = rotation * Vec3::new(0.0, 0.0, orbit.radius);

                    camera_transform.translation = focus_translation + offset;
                    camera_transform.look_at(focus_translation, Vec3::Y);
                }
            }
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
        mut cameras: Query<&mut crate::plugins::animation_player::components::OrbitalCamera>,
        time: Res<Time>,
    ) {
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
