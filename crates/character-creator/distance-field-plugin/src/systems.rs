use bevy::prelude::*;
use crate::components::*;

impl DistanceField {
    pub fn update(
        mut fields: Query<(Entity, &mut DistanceField, &GlobalTransform, Option<&Children>)>,
        shapes: Query<(&SdfShape, &GlobalTransform, Option<&SdfOperation>)>,
    ) {
        for (_entity, mut distance_field, field_transform, children) in fields.iter_mut() {
            distance_field.clear();

            let local_origin = Vec3::new(
                -(distance_field.width as f32) * distance_field.voxel_size / 2.0,
                -(distance_field.height as f32) * distance_field.voxel_size / 2.0,
                -(distance_field.depth as f32) * distance_field.voxel_size / 2.0
            );

            let mut relevant_shapes: Vec<(&SdfShape, &GlobalTransform, Option<&SdfOperation>)> = Vec::new();
            if let Some(children) = children {
                for child in children.iter() {
                    if let Ok(shape_data) = shapes.get(child) {
                        relevant_shapes.push(shape_data);
                    }
                }
            }

            // GlobalTransform extraction
            let (scale, rotation, translation) = field_transform.to_scale_rotation_translation();
            let field_to_world = Mat4::from_scale_rotation_translation(scale, rotation, translation);
            
            let shape_eval_data: Vec<_> = relevant_shapes.iter().map(|&(shape, shape_global, op)| {
                let (scale, rotation, translation) = shape_global.to_scale_rotation_translation();
                let shape_to_world = Mat4::from_scale_rotation_translation(scale, rotation, translation);
                let world_to_shape = shape_to_world.inverse();
                let field_to_shape = world_to_shape * field_to_world;
                (shape, field_to_shape, op.copied().unwrap_or(SdfOperation::Union))
            }).collect();

            for z in 0..distance_field.depth {
                for y in 0..distance_field.height {
                    for x in 0..distance_field.width {
                        let voxel_field_pos = local_origin + Vec3::new(
                            x as f32 * distance_field.voxel_size,
                            y as f32 * distance_field.voxel_size,
                            z as f32 * distance_field.voxel_size,
                        );

                        let mut voxel_val = f32::INFINITY;

                        for (shape, field_to_shape, op) in &shape_eval_data {
                            let local_pos = field_to_shape.transform_point3(voxel_field_pos);
                            
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
    }

    pub fn debug(
        fields: Query<(&DistanceField, &GlobalTransform)>,
        mut gizmos: Gizmos,
    ) {
        for (field, transform) in fields.iter() {
            let size = Vec3::new(
                field.width as f32 * field.voxel_size,
                field.height as f32 * field.voxel_size,
                field.depth as f32 * field.voxel_size,
            );
            
            let (_scale, rotation, translation) = transform.to_scale_rotation_translation();
            
            let gizmo_transform = Transform {
                translation,
                rotation,
                scale: size,
            };
            
            gizmos.cuboid(gizmo_transform, Color::srgb(1.0, 0.0, 1.0));
        }
    }
}