use adventuresim_building_generator::BuildingCollision;
use adventuresim_tactical_core::prelude::*;
use bevy::prelude::*;

pub(super) fn spawn_tactical_buildings(commands: &mut Commands, buildings: Vec<GeneratedBuilding>) {
    for building in buildings {
        let collision_centre = building.collision.bounds.centre();
        let local_floor_offset = collision_centre.y - building.collision.bounds.min.y;
        commands.spawn((
            Name::new(format!("Tactical building {}", building.placement.id)),
            SceneBuilding {
                id: building.placement.id,
                program: building.placement.program,
                quarter_turns: building.placement.quarter_turns,
            },
            RigidBody::Static,
            CollisionLayers::new(TACTICAL_TERRAIN_LAYER, LayerMask::ALL),
            tactical_building_collider(&building.collision),
            Transform::from_xyz(
                building.placement.centre_metres.x,
                building.pad_elevation_metres + local_floor_offset,
                building.placement.centre_metres.y,
            )
            .with_rotation(Quat::from_rotation_y(
                f32::from(building.placement.quarter_turns) * core::f32::consts::FRAC_PI_2,
            )),
        ));
    }
}

fn tactical_building_collider(collision: &BuildingCollision) -> Collider {
    let local_origin = collision.bounds.centre();
    Collider::compound(
        collision
            .cuboids
            .iter()
            .map(|cuboid| {
                let rotation = Quat::from_rotation_y(cuboid.yaw_radians)
                    * Quat::from_rotation_x(cuboid.crossfall_radians)
                    * Quat::from_rotation_z(cuboid.longfall_radians);
                (
                    cuboid.centre - local_origin,
                    rotation,
                    Collider::cuboid(cuboid.size.x, cuboid.size.y, cuboid.size.z),
                )
            })
            .collect(),
    )
}
