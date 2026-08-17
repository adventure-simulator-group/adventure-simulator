use crate::math::Transform;

/// The joint hierarchy, in momentum order (parents always precede children).
#[derive(Debug, Default, Clone)]
pub struct Skeleton {
    pub names: Vec<String>,
    /// Parent index, or `-1` for a root.
    pub parents: Vec<i32>,
    pub translation_offsets: Vec<[f32; 3]>,
    /// Rest rotation baked into the joint, `[x, y, z, w]`.
    pub prerotations: Vec<[f32; 4]>,
}

impl Skeleton {
    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub fn joint_index(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    /// Bind-pose global transform per joint (all joint parameters zero).
    pub fn bind_pose(&self) -> Vec<Transform> {
        let mut global: Vec<Transform> = Vec::with_capacity(self.len());
        for joint in 0..self.len() {
            let offset = self.translation_offsets[joint];
            let prerotation = self.prerotations[joint];
            let local = Transform {
                translation: [offset[0] as f64, offset[1] as f64, offset[2] as f64],
                rotation: [
                    prerotation[0] as f64,
                    prerotation[1] as f64,
                    prerotation[2] as f64,
                    prerotation[3] as f64,
                ],
                scale: 1.0,
            };
            let parent = self.parents[joint];
            global.push(if parent < 0 {
                local
            } else {
                global[parent as usize].compose(&local)
            });
        }
        global
    }
}
