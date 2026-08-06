use adventuresim_tactical_core::prelude::CharacterControllerCameraOf;
use bevy::prelude::*;

const THIRD_PERSON_DISTANCE: f32 = 3.5;
const THIRD_PERSON_HEIGHT: f32 = 0.25;

pub struct TacticalCameraPlugin;

impl Plugin for TacticalCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraMode>()
            .add_systems(Update, toggle_camera_mode)
            .add_systems(
                PostUpdate,
                apply_third_person_offset.before(TransformSystems::Propagate),
            );
    }
}

#[derive(Resource, Default)]
struct CameraMode {
    third_person: bool,
}

fn toggle_camera_mode(keyboard: Res<ButtonInput<KeyCode>>, mut mode: ResMut<CameraMode>) {
    if keyboard.just_pressed(KeyCode::F9) {
        mode.third_person = !mode.third_person;
        info!(
            third_person = mode.third_person,
            "Changed tactical camera mode"
        );
    }
}

fn apply_third_person_offset(
    mode: Res<CameraMode>,
    mut cameras: Query<&mut Transform, With<CharacterControllerCameraOf>>,
) {
    if !mode.third_person {
        return;
    }

    for mut transform in &mut cameras {
        let rotation = transform.rotation;
        transform.translation += third_person_offset(rotation);
    }
}

fn third_person_offset(rotation: Quat) -> Vec3 {
    let back = rotation * Vec3::Z;
    let horizontal_back = Vec3::new(back.x, 0.0, back.z)
        .try_normalize()
        .unwrap_or(Vec3::Z);
    horizontal_back * THIRD_PERSON_DISTANCE + Vec3::Y * THIRD_PERSON_HEIGHT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f9_toggles_third_person_mode() {
        let mut world = World::new();
        let mut keyboard = ButtonInput::default();
        keyboard.press(KeyCode::F9);
        world.insert_resource(keyboard);
        world.insert_resource(CameraMode::default());

        world.run_system_cached(toggle_camera_mode).unwrap();

        assert!(world.resource::<CameraMode>().third_person);
    }

    #[test]
    fn third_person_offset_keeps_distance_independent_of_pitch() {
        let rotation = Quat::from_euler(EulerRot::YXZ, 0.7, 1.2, 0.0);
        let offset = third_person_offset(rotation);

        assert!((offset.y - THIRD_PERSON_HEIGHT).abs() < 0.0001);
        assert!((offset.xz().length() - THIRD_PERSON_DISTANCE).abs() < 0.0001);
    }
}
