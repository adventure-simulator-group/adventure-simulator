//! Presentation for replicated server-authoritative door leaves.

use bevy::{math::primitives::Cuboid, prelude::*};
use bevy_mod_outline::{OutlineMode, OutlineVolume};

use super::{BuildingRenderLevel, SceneDoor, TacticalBuildingMaterials, building_lod_visibility};
use adventuresim_building_generator::BuildingLodMaterial;

#[derive(Component)]
pub(crate) struct PresentedDoorLeaf;

#[derive(Component)]
pub(crate) struct GrabTargetOutline(pub(crate) Entity);

pub(crate) struct DoorPresentationPlugin;

impl Plugin for DoorPresentationPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_scene_door_added);
    }
}

pub(in crate::presentation) fn on_scene_door_added(
    event: On<Add, SceneDoor>,
    mut commands: Commands,
    doors: Query<&SceneDoor>,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<TacticalBuildingMaterials>,
) -> Result {
    let door = doors.get(event.entity)?;
    commands.entity(event.entity).insert((
        PresentedDoorLeaf,
        Mesh3d(meshes.add(Cuboid::from_size(door.size_metres))),
        MeshMaterial3d(materials.get_for_building(door.building_id, BuildingLodMaterial::Timber)),
        Visibility::default(),
        building_lod_visibility(BuildingRenderLevel::Lod0),
        GrabTargetOutline(event.entity),
        OutlineVolume {
            visible: false,
            colour: Color::WHITE,
            width: 4.0,
        },
        OutlineMode::FloodFlat,
    ));
    Ok(())
}
