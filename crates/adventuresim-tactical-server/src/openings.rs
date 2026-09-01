//! Composition boundary for interactive building closures.

use adventuresim_building_generator::{
    BuildingCollision, BuildingPlan, compile_building_collision, generate as generate_building,
};
use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::bevy_replicon::prelude::Replicated;
use bevy::prelude::*;

#[path = "doors.rs"]
mod doors;
#[path = "windows.rs"]
mod windows;

pub(crate) use doors::DoorGrabber;
pub(crate) use windows::WindowGrabber;

pub(crate) struct BuildingOpeningsPlugin;

impl Plugin for BuildingOpeningsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((doors::DoorServerPlugin, windows::WindowServerPlugin));
    }
}

pub(crate) fn spawn_building_openings(
    commands: &mut Commands,
    building_entity: Entity,
    building: &SceneBuilding,
    transform: &Transform,
    plan: &BuildingPlan,
    collision: &BuildingCollision,
) {
    doors::spawn_building_doors(
        commands,
        building_entity,
        building,
        transform,
        plan,
        collision,
    );
    windows::spawn_building_windows(commands, building, transform, plan, collision);
}

pub(crate) fn on_scene_building_added(
    event: On<Add, SceneBuilding>,
    mut commands: Commands,
    buildings: Query<(&SceneBuilding, &Transform)>,
) -> Result {
    let (building, transform) = buildings.get(event.entity)?;
    let plan = generate_building(&building.program)?;
    let collision = compile_building_collision(&plan);
    spawn_building_openings(
        &mut commands,
        event.entity,
        building,
        transform,
        &plan,
        &collision,
    );
    commands.entity(event.entity).insert((
        Replicated,
        RigidBody::Static,
        CollisionLayers::new(TACTICAL_TERRAIN_LAYER, LayerMask::ALL),
        tactical_building_collider(&collision),
    ));
    Ok(())
}

fn tactical_building_collider(collision: &BuildingCollision) -> Collider {
    let local_origin = collision.bounds.centre();
    Collider::compound(
        collision
            .cuboids
            .iter()
            .map(|cuboid| {
                let translation = cuboid.centre - local_origin;
                let rotation = Quat::from_rotation_y(cuboid.yaw_radians)
                    * Quat::from_rotation_x(cuboid.crossfall_radians)
                    * Quat::from_rotation_z(cuboid.longfall_radians);
                (
                    translation,
                    rotation,
                    Collider::cuboid(cuboid.size.x, cuboid.size.y, cuboid.size.z),
                )
            })
            .collect(),
    )
}

pub(crate) fn spawn_generated_buildings(
    commands: &mut Commands,
    buildings: Vec<GeneratedBuilding>,
) {
    for building in buildings {
        let collision_centre = building.collision.bounds.centre();
        let local_floor_offset = collision_centre.y - building.collision.bounds.min.y;
        commands.spawn((
            Name::new(format!("Tactical building {}", building.placement.id)),
            SceneBuilding {
                id: building.placement.id,
                program: building.placement.program,
                orientation: building.placement.orientation,
            },
            Transform::from_xyz(
                building.placement.centre_metres.x,
                building.pad_elevation_metres + local_floor_offset,
                building.placement.centre_metres.y,
            )
            .with_rotation(Quat::from_rotation_y(
                building.placement.orientation.yaw_radians(),
            )),
        ));
    }
}
