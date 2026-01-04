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

    // Parallelize? For now, simple loop
    for z in 0..config.depth {
        for y in 0..config.height {
            for x in 0..config.width {
                let world_pos = origin + Vec3::new(
                    x as f32 * config.voxel_size,
                    y as f32 * config.voxel_size,
                    z as f32 * config.voxel_size,
                );

                // let mut current_dist = f32::INFINITY; 
                // Actually, standard marching cubes usually assumes a union of everything implies the surface.
                // But with CSG, order matters or we need a specific way to combine.
                // For a loose collection of shapes, "Union" is the default.
                
                // Simplified CSG: 
                // We'll iterate all shapes and apply them to the field.
                // But wait, `distance_field` is our canvas.
                // If we initialize it to INFINITY, we can Union (min) everything.
                // For subtraction, we need to operate on the existing value.
                
                // Let's grab the current value at this voxel
                let mut voxel_val = distance_field.get(x, y, z).clone();
                if voxel_val == f32::INFINITY {
                     // If it's fresh, we treat it as "outside"
                     // Ideally we want to constructive solid geometry (CSG).
                     // Simple approach: Union all simple shapes first?
                     // Or just iterate linear order.
                }

                for (shape, transform, op) in shapes.iter() {
                    // Convert world_pos to local space of the shape
                    let local_pos = transform.to_matrix().inverse().transform_point3(world_pos);
                    
                    let shape_dist = match shape {
                        SdfShape::Sphere { radius } => {
                            local_pos.length() - radius
                        },
                        SdfShape::Box { size } => {
                            let d = local_pos.abs() - *size;
                            d.max(Vec3::ZERO).length() + d.x.max(d.y).max(d.z).min(0.0)
                        }
                    };
                    
                    // Apply global scale (uniform only approximation)
                    // let scale = transform.scale.max_element(); 
                    // let final_dist = shape_dist * scale; 
                    // Correct SDF scaling is tricky with non-uniform scale. 
                    // Let's assume uniform scale or no scale for MVP.
                    
                    let op = op.unwrap_or(&SdfOperation::Union);
                    
                    match op {
                        SdfOperation::Union => {
                            voxel_val = voxel_val.min(shape_dist);
                        }
                        SdfOperation::Intersection => {
                            // Intersecting with infinity (initial) is bad if we start there.
                            // If voxel_val is INF, and we Intersect, we get shape_dist?
                            // Standard CSG: max(d1, d2).
                            // If d1 is INF, result is INF (empty).
                            // So intersection only makes sense if there is *something* there.
                            // For MVP, lets assume Union is the base, and we iterate.
                             if voxel_val == f32::INFINITY {
                                 voxel_val = shape_dist;
                             } else {
                                 voxel_val = voxel_val.max(shape_dist);
                             }
                        }
                        SdfOperation::Subtraction => {
                             // max(d1, -d2)
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
