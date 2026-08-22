use crate::skeleton::Skeleton;
use fabelgeist_math::matrix::Mat4;
use fabelgeist_math::vector::Vec3;

pub struct AutoRigger;

impl AutoRigger {
    pub fn fit_skeleton_to_mesh(skeleton: &mut Skeleton, vertices: &[Vec3]) {
        // Inverse bind matrices use model space, including the skeleton root.
        // Convert them back to skeleton-local world space while fitting.
        let skeleton_root = skeleton.transform.to_mat4();
        let inverse_skeleton_root = skeleton_root.inverse().unwrap_or(Mat4::identity());
        let mut world_positions = vec![Vec3::new(0.0, 0.0, 0.0); skeleton.joints.len()];
        for (world_position, joint) in world_positions.iter_mut().zip(&skeleton.joints) {
            let inv_bind = joint.inverse_bind_matrix;
            let model_world = inv_bind.inverse().unwrap_or(Mat4::identity());
            let world = inverse_skeleton_root * model_world;
            *world_position = Vec3::new(
                world.columns[3][0],
                world.columns[3][1],
                world.columns[3][2],
            );
        }

        // Refine positions toward nearby vertex centroids
        for _ in 0..2 {
            for i in 0..skeleton.joints.len() {
                if skeleton.joints[i].parent_index.is_none() {
                    continue;
                } // Keep root stable

                let current_pos = world_positions[i];

                let mut min_dist = 0.2f32;
                if let Some(parent_idx) = skeleton.joints[i].parent_index {
                    min_dist = (current_pos - world_positions[parent_idx]).length() * 0.5;
                }

                let radius = min_dist.clamp(0.05, 0.2);
                let radius_sq = radius * radius;

                let mut centroid = Vec3::new(0.0, 0.0, 0.0);
                let mut count = 0;

                for &v in vertices {
                    let d2 = (v - current_pos).length_sq();
                    if d2 < radius_sq {
                        centroid = centroid + v;
                        count += 1;
                    }
                }

                if count > 10 {
                    let target = centroid * (1.0 / count as f32);
                    world_positions[i] = current_pos + (target - current_pos) * 0.8;
                }
            }
        }

        // Rebuild local transforms and inverse bind matrices from new world positions
        use fabelgeist_math::transform::Transform;
        let mut world_transforms = vec![Mat4::identity(); skeleton.joints.len()];
        for i in 0..skeleton.joints.len() {
            world_transforms[i] = Mat4::from_translation(world_positions[i]);
        }

        for i in 0..skeleton.joints.len() {
            let world = world_transforms[i];
            let parent_world = skeleton.joints[i]
                .parent_index
                .map(|idx| world_transforms[idx])
                .unwrap_or(Mat4::identity());

            let local_mat = parent_world.inverse().unwrap_or(Mat4::identity()) * world;
            skeleton.joints[i].local_transform = Transform::from_mat4(local_mat);
            skeleton.joints[i].inverse_bind_matrix = (skeleton_root * world)
                .inverse()
                .unwrap_or(Mat4::identity());
        }

        // 3. Estimate the initial radius of each joint's shape based on nearest mesh vertices
        let mut joint_distances = vec![Vec::new(); skeleton.joints.len()];

        // Precompute children for each joint to avoid nested loops inside vertex iteration
        let mut children = vec![Vec::new(); skeleton.joints.len()];
        for (idx, joint) in skeleton.joints.iter().enumerate() {
            if let Some(p_idx) = joint.parent_index
                && p_idx < children.len()
            {
                children[p_idx].push(idx);
            }
        }

        for &v in vertices {
            let mut min_dist = f32::MAX;
            let mut best_idx = 0;

            for i in 0..skeleton.joints.len() {
                let current_pos = world_positions[i];
                let joint_children = &children[i];

                let dist = if !joint_children.is_empty() {
                    let mut min_child_dist = f32::MAX;
                    for &c_idx in joint_children {
                        let a = current_pos;
                        let b = world_positions[c_idx];
                        let ab = b - a;
                        let ap = v - a;
                        let ab_len_sq = ab.length_sq();
                        let d = if ab_len_sq < 1e-6 {
                            (v - a).length()
                        } else {
                            let t = (ap.dot(ab) / ab_len_sq).clamp(0.0, 1.0);
                            let projection = a + ab * t;
                            (v - projection).length()
                        };
                        if d < min_child_dist {
                            min_child_dist = d;
                        }
                    }
                    min_child_dist
                } else {
                    (v - current_pos).length()
                };

                if dist < min_dist {
                    min_dist = dist;
                    best_idx = i;
                }
            }

            joint_distances[best_idx].push(min_dist);
        }

        for (joint, dists) in skeleton.joints.iter_mut().zip(&joint_distances) {
            if !dists.is_empty() {
                let sum: f32 = dists.iter().sum();
                let avg = sum / dists.len() as f32;
                // Clamp between a thin finger (0.015) and a thick torso (0.25)
                let r = avg.clamp(0.015, 0.25);
                joint.radius = r;
                joint.smoothstep_start = 0.0;
                joint.smoothstep_end = r * 1.5;
            } else {
                let r = 0.08f32;
                joint.radius = r;
                joint.smoothstep_start = 0.0;
                joint.smoothstep_end = r * 1.5;
            }
        }
    }

    /// Computes skinning weights for a set of positions given a skeleton using distance-field based shapes.
    pub fn rig_positions(
        positions: &[Vec3],
        _indices: Option<&[u32]>,
        skeleton: &Skeleton,
        joint_positions: &[Vec3],
    ) -> (Vec<[u32; 4]>, Vec<[f32; 4]>) {
        let n_vertices = positions.len();
        if n_vertices == 0 {
            return (Vec::new(), Vec::new());
        }

        use crate::skeleton::ShapeType;

        // 1. Identify deforming joints (GPU joint indices) and their properties
        let has_any_joint_index = skeleton.joints.iter().any(|j| j.joint_index.is_some());
        let mut deforming_joints = Vec::new();
        for i in 0..skeleton.joints.len() {
            if !skeleton.joints[i].enabled {
                continue;
            }
            let gpu_idx_opt = if has_any_joint_index {
                skeleton.joints[i].joint_index
            } else {
                Some(i)
            };
            if let Some(gpu_idx) = gpu_idx_opt {
                deforming_joints.push((
                    gpu_idx,
                    i,
                    skeleton.joints[i].radius,
                    skeleton.joints[i].shape_type,
                    skeleton.joints[i].smoothstep_start,
                    skeleton.joints[i].smoothstep_end,
                ));
            }
        }

        if deforming_joints.is_empty() {
            return (
                vec![[0; 4]; n_vertices],
                vec![[0.0, 0.0, 0.0, 0.0]; n_vertices],
            );
        }

        // Precompute children for each joint to avoid nested loops inside vertex iteration
        let mut children = vec![Vec::new(); skeleton.joints.len()];
        for (idx, joint) in skeleton.joints.iter().enumerate() {
            if let Some(p_idx) = joint.parent_index
                && p_idx < children.len()
            {
                children[p_idx].push(idx);
            }
        }

        // 2. For each vertex, compute smooth blending weights based on the joint/bone shapes and smoothstep ranges
        let mut joints_out = Vec::with_capacity(n_vertices);
        let mut weights_out = Vec::with_capacity(n_vertices);

        for &p in positions {
            let mut influences = Vec::with_capacity(deforming_joints.len());

            for &(gpu_idx, joint_idx, radius, shape_type, ss_start, ss_end) in &deforming_joints {
                let current_pos = joint_positions[joint_idx];
                let joint_children = &children[joint_idx];

                let raw_dist = match shape_type {
                    ShapeType::Sphere => (p - current_pos).length(),
                    ShapeType::Capsule => {
                        if !joint_children.is_empty() {
                            let mut min_child_dist = f32::MAX;
                            for &c_idx in joint_children {
                                let a = current_pos;
                                let b = joint_positions[c_idx];
                                let ab = b - a;
                                let ap = p - a;
                                let ab_len_sq = ab.length_sq();
                                let d = if ab_len_sq < 1e-6 {
                                    (p - a).length()
                                } else {
                                    let t = (ap.dot(ab) / ab_len_sq).clamp(0.0, 1.0);
                                    let projection = a + ab * t;
                                    (p - projection).length()
                                };
                                if d < min_child_dist {
                                    min_child_dist = d;
                                }
                            }
                            min_child_dist
                        } else {
                            // Fallback to sphere if no children
                            (p - current_pos).length()
                        }
                    }
                };

                let dist = raw_dist - radius;

                // Compute smoothstep falloff: 1.0 at smoothstep_start, 0.0 at smoothstep_end
                let t = if ss_end > ss_start {
                    ((dist - ss_start) / (ss_end - ss_start)).clamp(0.0, 1.0)
                } else {
                    if dist <= ss_start { 0.0 } else { 1.0 }
                };
                let w = 1.0 - (t * t * (3.0 - 2.0 * t));
                let joint_dist = (p - current_pos).length();
                influences.push((gpu_idx, dist, raw_dist, joint_dist, w));
            }

            // Sort influences by weight descending, then by physical raw_dist ascending, then by joint_dist ascending
            influences.sort_by(|a, b| {
                b.4.partial_cmp(&a.4)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
                    .then_with(|| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal))
            });

            // Check if the vertex is inside the "core" of any joints (dist <= ss_start)
            let mut inside_cores = Vec::new();
            for &(gpu_idx, dist, raw_dist, joint_dist, _) in &influences {
                let ss_start = deforming_joints.iter().find(|x| x.0 == gpu_idx).unwrap().4;
                if dist <= ss_start {
                    inside_cores.push((gpu_idx, dist, raw_dist, joint_dist));
                }
            }

            let mut top_joints = [0u32; 4];
            let mut top_weights = [0.0f32; 4];

            if !inside_cores.is_empty() {
                // If it is inside the core of at least one shape, assign 100% weight to the physically closest one (smallest raw_dist, then smallest joint_dist)
                inside_cores.sort_by(|a, b| {
                    a.2.partial_cmp(&b.2)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal))
                });
                top_joints[0] = inside_cores[0].0 as u32;
                top_weights[0] = 1.0;
            } else {
                // Otherwise, use the standard normalized smoothstep falloff blending
                let mut total_weight = 0.0;
                for i in 0..4 {
                    if i < influences.len() {
                        let (gpu_idx, _, _, _, w) = influences[i];
                        top_joints[i] = gpu_idx as u32;
                        top_weights[i] = w;
                        total_weight += w;
                    }
                }

                // Normalize
                if total_weight > 1.0 {
                    for w in &mut top_weights {
                        *w /= total_weight;
                    }
                }
            }

            joints_out.push(top_joints);
            weights_out.push(top_weights);
        }

        (joints_out, weights_out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeleton::mixamo::MixamoRig;

    fn create_test_skeleton() -> Skeleton {
        MixamoRig::skeleton()
    }

    #[test]
    fn test_rig_positions() {
        let skeleton = create_test_skeleton();
        let joint_positions = skeleton.world_positions();

        let positions = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.1, 0.5, 0.0),
            Vec3::new(-0.1, 0.5, 0.0),
        ];

        let (joints, weights) =
            AutoRigger::rig_positions(&positions, None, &skeleton, &joint_positions);

        assert_eq!(joints.len(), 3);
        assert_eq!(weights.len(), 3);

        for w in weights {
            let sum: f32 = w.iter().sum();
            assert!((0.0..=1.0 + 1e-4).contains(&sum));
        }
    }

    #[test]
    fn fit_skeleton_preserves_root_in_inverse_bind_space() {
        use crate::skeleton::Joint;
        use fabelgeist_math::transform::Transform;

        let skeleton_root = Transform::from_scale(Vec3::new(0.01, 0.01, 0.01));
        let joint_world = Transform::from_position(Vec3::new(0.0, 100.0, 0.0)).to_mat4();
        let inverse_bind = (skeleton_root.to_mat4() * joint_world)
            .inverse()
            .expect("bind transform must be invertible");
        let mut skeleton = Skeleton::new(vec![Joint::new(
            "Root".to_string(),
            0,
            None,
            inverse_bind,
            Transform::from_mat4(joint_world),
            Some(0),
        )]);
        skeleton.transform = skeleton_root;

        AutoRigger::fit_skeleton_to_mesh(&mut skeleton, &[]);

        let fitted_world = skeleton.joints[0].local_transform.to_mat4();
        let bind_pose_skin_matrix =
            skeleton.transform.to_mat4() * fitted_world * skeleton.joints[0].inverse_bind_matrix;
        for column in 0..4 {
            for row in 0..4 {
                let expected = Mat4::identity().columns[column][row];
                assert!(
                    (bind_pose_skin_matrix.columns[column][row] - expected).abs() < 1.0e-4,
                    "matrix differs at column {column}, row {row}"
                );
            }
        }
    }

    #[test]
    fn test_rig_positions_gpu_indices() {
        use crate::skeleton::Joint;
        use fabelgeist_math::transform::Transform;

        // Create a custom skeleton:
        // - Joint 0 (Root): position at (0, 0, 0), parent = None, joint_index = None (non-deforming)
        // - Joint 1 (Spine): position at (0, 1, 0), parent = Some(0), joint_index = Some(5) (deforming, GPU idx = 5)
        // - Joint 2 (Head): position at (0, 2, 0), parent = Some(1), joint_index = Some(2) (deforming, GPU idx = 2)
        let root = Joint::new(
            "Root".to_string(),
            0,
            None,
            Mat4::identity(),
            Transform::default(),
            None,
        );
        let spine = Joint::new(
            "Spine".to_string(),
            1,
            Some(0),
            Mat4::from_translation(Vec3::new(0.0, -1.0, 0.0)),
            Transform::from_position(Vec3::new(0.0, 1.0, 0.0)),
            Some(5),
        );
        let head = Joint::new(
            "Head".to_string(),
            2,
            Some(1),
            Mat4::from_translation(Vec3::new(0.0, -2.0, 0.0)),
            Transform::from_position(Vec3::new(0.0, 1.0, 0.0)),
            Some(2),
        );

        let skeleton = Skeleton::new(vec![root, spine, head]);
        let joint_positions = skeleton.world_positions();

        // Check world positions are correct: Root = (0,0,0), Spine = (0,1,0), Head = (0,2,0)
        assert_eq!(joint_positions[0], Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(joint_positions[1], Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(joint_positions[2], Vec3::new(0.0, 2.0, 0.0));

        // Vertex positions to test:
        // 1. A vertex exactly at Head (0, 2, 0)
        // 2. A vertex on the Root-Spine bone (0, 0.8, 0), closer to Spine
        let positions = vec![Vec3::new(0.0, 2.0, 0.0), Vec3::new(0.0, 0.9, 0.0)];

        let (joints, weights) =
            AutoRigger::rig_positions(&positions, None, &skeleton, &joint_positions);

        assert_eq!(joints.len(), 2);
        assert_eq!(weights.len(), 2);

        // Vertex 1 is closest to Head (joint_index = 2).
        // Since Root has joint_index = None, it should not be present in the influences.
        // Therefore, the primary influence (highest weight) should be GPU joint index 2.
        assert_eq!(joints[0][0], 2);
        assert!(weights[0][0] > 0.9);

        // Vertex 2 is on Root-Spine bone whose child is Spine (joint_index = 5).
        // Since Root has joint_index = None, the primary influence should be GPU joint index 5.
        assert_eq!(joints[1][0], 5);
        assert!(weights[1][0] > 0.9);

        // Ensure no topological indices (like 0, 1, 2) that don't have joint_index are used.
        // In this case, 0 (Root) should never be in the influences.
        // Let's verify that the output joint indices are strictly either 2 or 5.
        // (Note: unused slots will be initialized to 0, but their weight should be 0.0)
        for i in 0..2 {
            for j in 0..4 {
                let j_idx = joints[i][j];
                let j_weight = weights[i][j];
                if j_weight > 0.0 {
                    assert!(j_idx == 2 || j_idx == 5);
                }
            }
        }
    }

    #[test]
    fn test_single_bone_falloff() {
        use crate::skeleton::Joint;
        use fabelgeist_math::transform::Transform;

        let mut joint = Joint::new(
            "Bone".to_string(),
            0,
            None,
            Mat4::identity(),
            Transform::default(),
            Some(0),
        );
        joint.radius = 0.1;
        joint.smoothstep_start = 0.0;
        joint.smoothstep_end = 0.5;
        joint.shape_type = crate::skeleton::ShapeType::Sphere;

        let skeleton = Skeleton::new(vec![joint]);
        let joint_positions = skeleton.world_positions();

        // Test three vertices:
        // 1. Inside core (raw_dist = 0.05 <= radius 0.1 + start 0.0) -> weight = 1.0
        // 2. In falloff zone (raw_dist = 0.35, dist = 0.25 halfway between 0.0 and 0.5) -> weight ~ 0.5
        // 3. Outside shape end (raw_dist = 0.65, dist = 0.55 > end 0.5) -> weight = 0.0
        let positions = vec![
            Vec3::new(0.05, 0.0, 0.0),
            Vec3::new(0.35, 0.0, 0.0),
            Vec3::new(0.65, 0.0, 0.0),
        ];

        let (_joints, weights) =
            AutoRigger::rig_positions(&positions, None, &skeleton, &joint_positions);

        // Position 1: Inside core, full weight
        assert_eq!(weights[0][0], 1.0);

        // Position 2: In falloff region, weight affected by smoothstep start and end
        assert!(
            weights[1][0] > 0.4 && weights[1][0] < 0.6,
            "Expected weight in falloff zone to be around 0.5, got {}",
            weights[1][0]
        );

        // Position 3: Outside falloff end, zero weight
        assert_eq!(weights[2][0], 0.0);
    }
}
