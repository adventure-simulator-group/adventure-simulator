use super::*;

pub(super) fn vista_valley(state: &SceneCaptureState, half: f32) -> (Vec3, Vec3, Vec3) {
    let direction = (state.valley_target.xz() - state.obstacle_focus.xz())
        .try_normalize()
        .unwrap_or(-Vec2::X);
    let safe_height = state
        .terrain
        .maximum_height_metres
        .max(state.vista_peak_metres)
        + 18.0;
    let mut position = state.obstacle_focus
        - Vec3::new(direction.x, 0.0, direction.y) * (half * 0.62)
        + Vec3::Y * 8.0;
    position.y = position.y.max(safe_height);
    let mut target = state.obstacle_focus
        + Vec3::new(direction.x, 0.0, direction.y) * (half * 5.0)
        + Vec3::Y * 2.0;
    target.y = target.y.clamp(position.y - 80.0, position.y - 8.0);
    (position, target, Vec3::Y)
}
