use bevy::prelude::*;

/// Selects the in-range candidate closest to the camera's forward axis.
/// Distance only breaks equal cursor alignment, matching ground-item auto aim.
pub(crate) fn auto_aim_candidate<T: Copy>(
    camera_origin: Vec3,
    camera_forward: Vec3,
    actor_origin: Vec3,
    maximum_distance: f32,
    candidates: impl IntoIterator<Item = (T, Vec3, u64)>,
) -> Option<T> {
    let camera_forward = camera_forward.try_normalize()?;
    let maximum_distance_squared = maximum_distance.max(0.0).powi(2);
    candidates
        .into_iter()
        .filter_map(|(candidate, position, stable_key)| {
            let actor_distance_squared = position.distance_squared(actor_origin);
            if actor_distance_squared > maximum_distance_squared {
                return None;
            }
            let alignment = (position - camera_origin)
                .try_normalize()
                .map_or(-1.0, |direction| direction.dot(camera_forward));
            let angular_error = 1.0 - alignment.clamp(-1.0, 1.0);
            Some((candidate, angular_error, actor_distance_squared, stable_key))
        })
        .min_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then(left.2.total_cmp(&right.2))
                .then(left.3.cmp(&right.3))
        })
        .map(|(candidate, _, _, _)| candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_aim_prefers_cursor_alignment_over_character_distance() {
        assert_eq!(
            auto_aim_candidate(
                Vec3::Y,
                Vec3::NEG_Z,
                Vec3::ZERO,
                2.0,
                [
                    ("nearby side", Vec3::new(0.5, 0.0, 0.0), 1),
                    ("pointed", Vec3::new(0.0, 0.0, -1.8), 2),
                ],
            ),
            Some("pointed")
        );
    }

    #[test]
    fn auto_aim_has_no_cursor_cone() {
        assert_eq!(
            auto_aim_candidate(
                Vec3::Y,
                Vec3::NEG_Z,
                Vec3::ZERO,
                2.0,
                [("behind", Vec3::new(0.0, 0.0, 1.5), 1)],
            ),
            Some("behind")
        );
    }

    #[test]
    fn auto_aim_excludes_candidates_outside_range() {
        assert_eq!(
            auto_aim_candidate(
                Vec3::Y,
                Vec3::NEG_Z,
                Vec3::ZERO,
                2.0,
                [("far", Vec3::new(0.0, 0.0, -2.01), 1)],
            ),
            None
        );
    }
}
