use bevy::prelude::*;
use crate::components::*;
use crate::field::*;

#[derive(Resource)]
pub struct SdfConfig {
    pub width: usize,
    pub height: usize,
    pub depth: usize,
    pub voxel_size: f32,
    // Center of the grid in world space
    pub center: Vec3,
}

impl Default for SdfConfig {
    fn default() -> Self {
        Self {
            width: 36,
            height: 36,
            depth: 36,
            voxel_size: 0.12,
            center: Vec3::ZERO,
        }
    }
}

pub fn update_distance_field(
    config: Res<SdfConfig>,
    mut distance_field: ResMut<DistanceField>,
    shapes: Query<(&SdfShape, &Transform, Option<&SdfOperation>)>,
) {
    // Reset field to infinity
    for val in distance_field.iter_mut() {
        *val = f32::INFINITY;
    }

    let origin = config.center - Vec3::new(
        config.width as f32 * config.voxel_size / 2.0,
        config.height as f32 * config.voxel_size / 2.0,
        config.depth as f32 * config.voxel_size / 2.0
    );

    // Pre-calculate inverse transforms for all shapes
    let shape_data: Vec<_> = shapes.iter().map(|(shape, transform, op)| {
        (shape, transform.to_matrix().inverse(), op.unwrap_or(&SdfOperation::Union))
    }).collect();

    // Parallelize? For now, simple loop
    for z in 0..config.depth {
        for y in 0..config.height {
            for x in 0..config.width {
                let world_pos = origin + Vec3::new(
                    x as f32 * config.voxel_size,
                    y as f32 * config.voxel_size,
                    z as f32 * config.voxel_size,
                );

                // Initialize with infinity
                let mut voxel_val = f32::INFINITY;

                for (shape, inverse_transform, op) in &shape_data {
                    // Convert world_pos to local space of the shape
                    let local_pos = inverse_transform.transform_point3(world_pos);
                    
                    let shape_dist = match shape {
                        SdfShape::Sphere { radius } => {
                            local_pos.length() - radius
                        },
                        SdfShape::Box { size } => {
                            let d = local_pos.abs() - *size;
                            d.max(Vec3::ZERO).length() + d.x.max(d.y).max(d.z).min(0.0)
                        }
                    };
                    
                    match op {
                        SdfOperation::Union => {
                            voxel_val = voxel_val.min(shape_dist);
                        }
                        SdfOperation::Intersection => {
                             if voxel_val == f32::INFINITY {
                                 voxel_val = shape_dist;
                             } else {
                                 voxel_val = voxel_val.max(shape_dist);
                             }
                        }
                        SdfOperation::Subtraction => {
                             if voxel_val != f32::INFINITY {
                                 voxel_val = voxel_val.max(-shape_dist);
                             }
                        }
                    }
                }
                
                distance_field.set(x, y, z, voxel_val);
            }
        }
    }
}

pub fn debug_sdf(
    config: Res<SdfConfig>,
    mut gizmos: Gizmos,
) {
    let size = Vec3::new(
        config.width as f32 * config.voxel_size,
        config.height as f32 * config.voxel_size,
        config.depth as f32 * config.voxel_size,
    );
    gizmos.cuboid(Transform::from_translation(config.center).with_scale(size), Color::srgb(1.0, 0.0, 1.0));
}
