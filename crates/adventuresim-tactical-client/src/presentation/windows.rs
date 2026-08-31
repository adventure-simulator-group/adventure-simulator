//! Presentation for replicated server-authoritative window casements.

use adventuresim_building_generator::BuildingLodMaterial;
use bevy::{math::primitives::Cuboid, prelude::*};
use bevy_mod_outline::{OutlineMode, OutlineVolume};

use super::{
    BuildingRenderLevel, GrabTargetOutline, SceneWindow, TacticalBuildingMaterials,
    building_lod_visibility,
};

#[derive(Component)]
pub(crate) struct PresentedWindowCasement;

pub(crate) struct WindowPresentationPlugin;

impl Plugin for WindowPresentationPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(on_scene_window_added);
    }
}

fn on_scene_window_added(
    event: On<Add, SceneWindow>,
    mut commands: Commands,
    windows: Query<&SceneWindow>,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<TacticalBuildingMaterials>,
) -> Result {
    let window = windows.get(event.entity)?;
    commands.entity(event.entity).insert((
        PresentedWindowCasement,
        Mesh3d(meshes.add(Cuboid::from_size(window.size_metres))),
        MeshMaterial3d(materials.get(BuildingLodMaterial::Glass)),
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
