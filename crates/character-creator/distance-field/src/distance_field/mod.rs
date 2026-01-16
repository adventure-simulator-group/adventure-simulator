use bevy_math::{Vec3, Mat4};

use crate::Field;

pub type Distance = f32;
pub type DistanceField = Field<Distance>;

impl DistanceField {
    pub fn new_distance_field(width: usize, height: usize, depth: usize, voxel_size: f32) -> Self {
        Self::new(width, height, depth, voxel_size, Distance::INFINITY)
    }

    pub fn clear(&mut self) {
        self.set_all(Distance::INFINITY);
    }

    pub fn add_sphere(&mut self, center: Vec3, radius: f32, voxel_size: f32) {
        // Simple SDF addition (min)
        let (width, height, depth) = self.dimensions();
        let origin = Vec3::new(
            -(width as f32 - 1.0) * 0.5 * voxel_size,
            -(height as f32 - 1.0) * 0.5 * voxel_size,
            -(depth as f32 - 1.0) * 0.5 * voxel_size,
        );

        for z in 0..depth {
            for y in 0..height {
                for x in 0..width {
                    let world_position = origin + Vec3::new(
                        x as f32 * voxel_size,
                        y as f32 * voxel_size,
                        z as f32 * voxel_size,
                    );
                    let distance = world_position.distance(center) - radius;
                    let current = self.get(x, y, z);
                    self.set(x, y, z, current.min(distance));
                }
            }
        }
    }

    pub fn update(&mut self, shape_eval_data: &[(crate::SdfShape, Mat4, crate::SdfOperation)]) {
        let (width, height, depth) = self.dimensions();
        
        let local_origin = Vec3::new(
            -(width as f32) * self.voxel_size / 2.0,
            -(height as f32) * self.voxel_size / 2.0,
            -(depth as f32) * self.voxel_size / 2.0
        );

        for z in 0..depth {
            for y in 0..height {
                for x in 0..width {
                    let voxel_field_pos = local_origin + Vec3::new(
                        x as f32 * self.voxel_size,
                        y as f32 * self.voxel_size,
                        z as f32 * self.voxel_size,
                    );

                    let mut voxel_val = f32::INFINITY;

                    for (shape, field_to_shape, op) in shape_eval_data {
                        // transform_point3 is available in bevy_math::Mat4
                        let local_pos = field_to_shape.transform_point3(voxel_field_pos);

                        let shape_dist = match shape {
                            crate::SdfShape::Sphere(sphere) => {
                                local_pos.length() - sphere.radius
                            },
                            crate::SdfShape::Box(box_) => {
                                let d = local_pos.abs() - box_.size;
                                d.max(Vec3::ZERO).length() + d.x.max(d.y).max(d.z).min(0.0)
                            }
                        };
                        
                        match op {
                            crate::SdfOperation::Union => {
                                voxel_val = voxel_val.min(shape_dist);
                            }
                            crate::SdfOperation::Intersection => {
                                if voxel_val == f32::INFINITY {
                                    voxel_val = shape_dist;
                                } else {
                                    voxel_val = voxel_val.max(shape_dist);
                                }
                            }
                            crate::SdfOperation::Subtraction => {
                                if voxel_val != f32::INFINITY {
                                    voxel_val = voxel_val.max(-shape_dist);
                                }
                            }
                        }
                    }
                    
                    self.set(x, y, z, voxel_val);
                }
            }
        }
    }
}

