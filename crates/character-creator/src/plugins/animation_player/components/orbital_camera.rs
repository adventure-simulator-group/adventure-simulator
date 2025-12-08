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
}
