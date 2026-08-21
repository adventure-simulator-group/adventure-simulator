//! Quaternion and skeleton-state algebra on Burn tensors.
//!
//! A skeleton state is `[tx, ty, tz, qx, qy, qz, qw, s]` and represents a
//! rigid transform with uniform scale. All helpers work on rank-3 tensors
//! shaped `[batch, count, channels]`, which is every shape MHR needs.

use burn::tensor::Tensor;

/// One channel of the last dimension, keeping the rank.
fn channel(tensor: &Tensor<3>, index: usize) -> Tensor<3> {
    tensor.clone().narrow(2, index, 1)
}

/// The `[x, y, z]` part of a quaternion.
fn axis(quaternion: &Tensor<3>, offset: usize) -> Tensor<3> {
    quaternion.clone().narrow(2, offset, 3)
}

/// Hamilton product of two normalized quaternions, `[..., 4]`.
pub fn quaternion_multiply(a: Tensor<3>, b: Tensor<3>) -> Tensor<3> {
    let (x1, y1, z1, w1) = (
        channel(&a, 0),
        channel(&a, 1),
        channel(&a, 2),
        channel(&a, 3),
    );
    let (x2, y2, z2, w2) = (
        channel(&b, 0),
        channel(&b, 1),
        channel(&b, 2),
        channel(&b, 3),
    );

    let x = w1.clone() * x2.clone() + x1.clone() * w2.clone() + y1.clone() * z2.clone()
        - z1.clone() * y2.clone();
    let y = w1.clone() * y2.clone() - x1.clone() * z2.clone()
        + y1.clone() * w2.clone()
        + z1.clone() * x2.clone();
    let z = w1.clone() * z2.clone() + x1.clone() * y2.clone() - y1.clone() * x2.clone()
        + z1.clone() * w2.clone();
    let w = w1 * w2 - x1 * x2 - y1 * y2 - z1 * z2;

    Tensor::cat(vec![x, y, z, w], 2)
}

/// XYZ Euler angles (radians) to a quaternion, matching pymomentum's
/// `euler_xyz_to_quaternion` (the x rotation is applied first).
pub fn euler_xyz_to_quaternion(euler: Tensor<3>) -> Tensor<3> {
    let half = euler * 0.5;
    let (roll, pitch, yaw) = (channel(&half, 0), channel(&half, 1), channel(&half, 2));
    let (sr, cr) = (roll.clone().sin(), roll.cos());
    let (sp, cp) = (pitch.clone().sin(), pitch.cos());
    let (sy, cy) = (yaw.clone().sin(), yaw.cos());

    let x = sr.clone() * cp.clone() * cy.clone() - cr.clone() * sp.clone() * sy.clone();
    let y = cr.clone() * sp.clone() * cy.clone() + sr.clone() * cp.clone() * sy.clone();
    let z = cr.clone() * cp.clone() * sy.clone() - sr.clone() * sp.clone() * cy.clone();
    let w = cr * cp * cy + sr * sp * sy;

    Tensor::cat(vec![x, y, z, w], 2)
}

/// Rotates `points` by a normalized quaternion, `v + 2 * (r * (a x v) + a x (a x v))`.
pub fn quaternion_rotate(quaternion: &Tensor<3>, points: Tensor<3>) -> Tensor<3> {
    let scalar = channel(quaternion, 3);
    let vector = axis(quaternion, 0);
    let av = cross(vector.clone(), points.clone());
    let aav = cross(vector, av.clone());
    points + (av * scalar + aav) * 2.0
}

fn cross(a: Tensor<3>, b: Tensor<3>) -> Tensor<3> {
    let (ax, ay, az) = (channel(&a, 0), channel(&a, 1), channel(&a, 2));
    let (bx, by, bz) = (channel(&b, 0), channel(&b, 1), channel(&b, 2));
    Tensor::cat(
        vec![
            ay.clone() * bz.clone() - az.clone() * by.clone(),
            az * bx.clone() - ax.clone() * bz,
            ax * by - ay * bx,
        ],
        2,
    )
}

/// Splits a skeleton state into translation, rotation and scale.
pub fn split(state: &Tensor<3>) -> (Tensor<3>, Tensor<3>, Tensor<3>) {
    (
        state.clone().narrow(2, 0, 3),
        state.clone().narrow(2, 3, 4),
        state.clone().narrow(2, 7, 1),
    )
}

/// Composes two skeleton states, `s1 * s2`.
pub fn multiply(s1: Tensor<3>, s2: Tensor<3>) -> Tensor<3> {
    let (t1, q1, scale1) = split(&s1);
    let (t2, q2, scale2) = split(&s2);
    let translation = t1 + quaternion_rotate(&q1, t2) * scale1.clone();
    Tensor::cat(
        vec![translation, quaternion_multiply(q1, q2), scale1 * scale2],
        2,
    )
}

/// Applies a skeleton state to points, `t + q * (s * p)`.
pub fn transform_points(state: &Tensor<3>, points: Tensor<3>) -> Tensor<3> {
    let (translation, rotation, scale) = split(state);
    translation + quaternion_rotate(&rotation, points * scale)
}

/// Pointer-jumping levels that turn local states into global ones.
///
/// Level `l` multiplies every joint whose depth has bit `l` set by the
/// accumulated state of its ancestor at the depth prefix below that bit. This
/// is pymomentum's `calc_fk_prefix_multiplication_indices`, and it replaces a
/// 127-step serial walk with `ceil(log2(depth))` batched steps.
pub fn prefix_multiplication_levels(parents: &[i32]) -> Vec<(Vec<i32>, Vec<i32>)> {
    let chains: Vec<Vec<usize>> = (0..parents.len())
        .map(|joint| {
            let mut chain = vec![joint];
            let mut current = joint;
            while parents[current] >= 0 {
                current = parents[current] as usize;
                chain.push(current);
            }
            chain.reverse();
            chain
        })
        .collect();

    let mut levels = Vec::new();
    loop {
        let level = levels.len();
        let mut source = Vec::new();
        let mut target = Vec::new();
        for chain in &chains {
            let depth = chain.len() - 1;
            if (depth >> level) & 1 == 1 {
                source.push(chain[depth] as i32);
                target.push(chain[((depth >> level) << level) - 1] as i32);
            }
        }
        if source.is_empty() {
            break;
        }
        levels.push((source, target));
    }
    levels
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math;
    use burn::tensor::{Device, TensorData};

    fn tensor(values: Vec<f32>, shape: [usize; 3]) -> Tensor<3> {
        Tensor::from_data(TensorData::new(values, shape), &Device::default())
    }

    fn values(tensor: Tensor<3>) -> Vec<f32> {
        tensor.into_data().into_vec().unwrap()
    }

    fn assert_close(actual: &[f32], expected: &[f32], eps: f32) {
        assert_eq!(actual.len(), expected.len());
        for (a, b) in actual.iter().zip(expected) {
            assert!((a - b).abs() < eps, "{actual:?} != {expected:?}");
        }
    }

    #[test]
    fn quaternion_ops_match_the_host_implementation() {
        let a = math::quat_normalize([0.2, -0.5, 0.31, 0.77]);
        let b = math::quat_normalize([-0.1, 0.4, 0.2, 0.9]);
        let expected = math::quat_mul(a, b).map(|v| v as f32);

        let product = quaternion_multiply(
            tensor(a.iter().map(|v| *v as f32).collect(), [1, 1, 4]),
            tensor(b.iter().map(|v| *v as f32).collect(), [1, 1, 4]),
        );
        assert_close(&values(product), &expected, 1e-6);
    }

    #[test]
    fn euler_matches_the_host_implementation() {
        let angles = [0.3_f64, -0.7, 1.1];
        let expected = math::quat_from_euler(angles, [0, 1, 2]).map(|v| v as f32);
        let actual = euler_xyz_to_quaternion(tensor(
            angles.iter().map(|v| *v as f32).collect(),
            [1, 1, 3],
        ));
        assert_close(&values(actual), &expected, 1e-6);
    }

    #[test]
    fn transform_points_matches_the_host_implementation() {
        let transform = math::Transform {
            translation: [1.5, -2.0, 3.25],
            rotation: math::quat_normalize([0.2, -0.5, 0.31, 0.77]),
            scale: 1.7,
        };
        let point = [0.4_f64, 1.25, -0.5];
        let rotated = math::rotate_vector(transform.rotation, point.map(|v| v * transform.scale));
        let expected: Vec<f32> = (0..3)
            .map(|i| (transform.translation[i] + rotated[i]) as f32)
            .collect();

        let state = tensor(transform.to_skel_state().to_vec(), [1, 1, 8]);
        let actual = transform_points(&state, tensor(point.map(|v| v as f32).to_vec(), [1, 1, 3]));
        assert_close(&values(actual), &expected, 1e-5);
    }

    #[test]
    fn multiply_matches_the_host_implementation() {
        let a = math::Transform {
            translation: [1.5, -2.0, 3.25],
            rotation: math::quat_normalize([0.2, -0.5, 0.31, 0.77]),
            scale: 1.7,
        };
        let b = math::Transform {
            translation: [0.5, 4.0, -1.25],
            rotation: math::quat_normalize([-0.1, 0.4, 0.2, 0.9]),
            scale: 0.8,
        };
        let expected = a.compose(&b).to_skel_state();
        let actual = multiply(
            tensor(a.to_skel_state().to_vec(), [1, 1, 8]),
            tensor(b.to_skel_state().to_vec(), [1, 1, 8]),
        );
        assert_close(&values(actual), &expected, 1e-5);
    }

    #[test]
    fn prefix_levels_cover_every_ancestor_product() {
        // A chain of eight joints plus a second branch off joint 1.
        let parents = [-1, 0, 1, 2, 3, 4, 5, 6, 1];
        let levels = prefix_multiplication_levels(&parents);

        // Replay the scan on plain transforms and compare with a serial walk.
        let locals: Vec<math::Transform> = (0..parents.len())
            .map(|i| math::Transform {
                translation: [i as f64, 1.0, -0.5],
                rotation: math::quat_from_euler([0.1 * i as f64, 0.2, -0.05], [0, 1, 2]),
                scale: 1.0,
            })
            .collect();

        let mut serial: Vec<math::Transform> = Vec::new();
        for (joint, local) in locals.iter().enumerate() {
            let parent = parents[joint];
            serial.push(if parent < 0 {
                *local
            } else {
                serial[parent as usize].compose(local)
            });
        }

        let mut scanned = locals.clone();
        for (source, target) in &levels {
            let updated: Vec<math::Transform> = source
                .iter()
                .zip(target)
                .map(|(s, t)| scanned[*t as usize].compose(&scanned[*s as usize]))
                .collect();
            for (slot, value) in source.iter().zip(updated) {
                scanned[*slot as usize] = value;
            }
        }

        for (joint, (a, b)) in serial.iter().zip(&scanned).enumerate() {
            let (a, b) = (a.to_skel_state(), b.to_skel_state());
            for channel in 0..8 {
                assert!(
                    (a[channel] - b[channel]).abs() < 1e-9,
                    "joint {joint}: {a:?} != {b:?}"
                );
            }
        }
    }
}
