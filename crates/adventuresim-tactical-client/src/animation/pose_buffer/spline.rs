use super::*;

pub(super) fn clamped_cubic_spline_vec3<const KNOTS: usize>(
    knots: [f32; KNOTS],
    values: [Vec3; KNOTS],
    start_velocity: Vec3,
    end_velocity: Vec3,
    progress: f32,
) -> Vec3 {
    let second_derivatives =
        clamped_cubic_second_derivatives(knots, values, start_velocity, end_velocity);
    let progress = progress.clamp(knots[0], knots[KNOTS - 1]);
    let segment = (0..KNOTS - 1)
        .find(|segment| progress <= knots[segment + 1])
        .unwrap_or(KNOTS - 2);
    let start = knots[segment];
    let end = knots[segment + 1];
    let duration = end - start;
    let a = (end - progress) / duration;
    let b = (progress - start) / duration;
    values[segment] * a
        + values[segment + 1] * b
        + (second_derivatives[segment] * (a * a * a - a)
            + second_derivatives[segment + 1] * (b * b * b - b))
            * (duration * duration / 6.0)
}

fn clamped_cubic_second_derivatives<const KNOTS: usize>(
    knots: [f32; KNOTS],
    values: [Vec3; KNOTS],
    start_velocity: Vec3,
    end_velocity: Vec3,
) -> [Vec3; KNOTS] {
    assert!(KNOTS >= 2, "a spline needs at least two knots");
    let mut intervals = [0.0; KNOTS];
    for interval in 0..KNOTS - 1 {
        intervals[interval] = knots[interval + 1] - knots[interval];
    }
    let mut lower = [0.0; KNOTS];
    let mut diagonal = [0.0; KNOTS];
    let mut upper = [0.0; KNOTS];
    let mut right = [Vec3::ZERO; KNOTS];
    diagonal[0] = 2.0 * intervals[0];
    upper[0] = intervals[0];
    right[0] = 6.0 * ((values[1] - values[0]) / intervals[0] - start_velocity);
    for knot in 1..KNOTS - 1 {
        lower[knot] = intervals[knot - 1];
        diagonal[knot] = 2.0 * (intervals[knot - 1] + intervals[knot]);
        upper[knot] = intervals[knot];
        right[knot] = 6.0
            * ((values[knot + 1] - values[knot]) / intervals[knot]
                - (values[knot] - values[knot - 1]) / intervals[knot - 1]);
    }
    let last = KNOTS - 1;
    lower[last] = intervals[last - 1];
    diagonal[last] = 2.0 * intervals[last - 1];
    right[last] = 6.0 * (end_velocity - (values[last] - values[last - 1]) / intervals[last - 1]);
    for row in 1..KNOTS {
        let factor = lower[row] / diagonal[row - 1];
        diagonal[row] -= factor * upper[row - 1];
        right[row] -= right[row - 1] * factor;
    }
    let mut result = [Vec3::ZERO; KNOTS];
    result[last] = right[last] / diagonal[last];
    for row in (0..last).rev() {
        result[row] = (right[row] - result[row + 1] * upper[row]) / diagonal[row];
    }
    result
}
