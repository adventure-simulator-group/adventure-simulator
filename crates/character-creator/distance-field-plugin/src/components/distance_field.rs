use crate::components::*;
use crate::prelude::*;

pub use ::distance_field::DistanceField;

#[derive(Component, Shrinkwrap)]
#[shrinkwrap(mutable)]
pub struct DistanceFieldComponent(pub DistanceField);

#[derive(Component)]
pub struct StaticSdf;

impl DistanceFieldComponent {
    pub fn new(x: usize, y: usize, z: usize, voxel_size: f32) -> Self {
        Self(DistanceField::new_distance_field(x, y, z, voxel_size))
    }

    pub fn update(
        mut fields: Query<
            (
                Entity,
                &mut DistanceFieldComponent,
                &GlobalTransform,
                Option<&Children>,
            ),
            Without<StaticSdf>,
        >,
        shapes: Query<(
            &SdfShapeComponent,
            &GlobalTransform,
            Option<&SdfOperationComponent>,
        )>,
    ) {
        for (_entity, mut distance_field, field_transform, children) in fields.iter_mut() {
            let relevant_shapes: Vec<(
                &SdfShapeComponent,
                &GlobalTransform,
                Option<&SdfOperationComponent>,
            )> = if let Some(children) = children {
                children
                    .iter()
                    .filter_map(|child| shapes.get(child).ok())
                    .collect()
            } else {
                Vec::new()
            };

            // GlobalTransform extraction
            let (scale, rotation, translation) = field_transform.to_scale_rotation_translation();
            let field_to_world =
                Mat4::from_scale_rotation_translation(scale, rotation, translation);

            let shape_eval_data: Vec<_> = relevant_shapes
                .iter()
                .map(|&(shape, shape_global, op)| {
                    let (scale, rotation, translation) =
                        shape_global.to_scale_rotation_translation();
                    let shape_to_world =
                        Mat4::from_scale_rotation_translation(scale, rotation, translation);
                    let world_to_shape = shape_to_world.inverse();
                    let field_to_shape = world_to_shape * field_to_world;

                    let SdfShapeComponent(core_shape) = *shape;
                    let SdfOperationComponent(core_op) = op.copied().unwrap_or_default();

                    (core_shape, field_to_shape, core_op)
                })
                .collect();

            distance_field.update(&shape_eval_data);
        }
    }

    pub fn debug(
        fields: Query<(
            &DistanceFieldComponent,
            &GlobalTransform,
            Option<&Visibility>,
        )>,
        mut gizmos: Gizmos,
    ) {
        for (field, transform, visibility) in fields.iter() {
            if let Some(viz) = visibility {
                if viz == &Visibility::Hidden {
                    continue;
                }
            }

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
