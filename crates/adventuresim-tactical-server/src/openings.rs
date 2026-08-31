//! Composition boundary for interactive building closures.

use adventuresim_building_generator::{BuildingCollision, BuildingPlan};
use adventuresim_tactical_core::prelude::*;
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
