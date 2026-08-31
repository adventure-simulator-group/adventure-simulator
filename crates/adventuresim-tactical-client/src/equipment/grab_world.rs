//! World-space pickup and door targeting shared by grab input and outlines.

use adventuresim_tactical_core::prelude::*;
use bevy::{ecs::system::SystemParam, prelude::*};
use bevy_mod_outline::OutlineVolume;

use crate::{
    presentation::{GrabTargetOutline, TacticalGameplayCamera},
    targeting::auto_aim_candidate,
};

use super::{GrabSelection, GrabSession, PICKUP_RANGE_M};

#[derive(SystemParam)]
pub(super) struct WorldGrabTargets<'w, 's> {
    cameras: Query<'w, 's, &'static GlobalTransform, With<TacticalGameplayCamera>>,
    scene_items: Query<
        'w,
        's,
        (
            Entity,
            &'static GlobalTransform,
            &'static TacticalEquipmentPhysical,
        ),
        With<TacticalSceneItem>,
    >,
    doors: Query<'w, 's, (Entity, &'static GlobalTransform, &'static SceneDoor)>,
    spatial: SpatialQuery<'w, 's>,
}

impl WorldGrabTargets<'_, '_> {
    pub(super) fn pointed(&self, actor: &GlobalTransform) -> Option<GrabSelection> {
        let camera = self.cameras.single().ok()?;
        let origin = actor.translation() + Vec3::Y * 0.6;
        auto_aim_candidate(
            camera.translation(),
            camera.forward().as_vec3(),
            actor.translation(),
            PICKUP_RANGE_M,
            self.scene_items
                .iter()
                .filter_map(|(entity, transform, physical)| {
                    let position = transform.transform_point(-physical.anchor_offset_m);
                    self.visible(origin, position).then_some((
                        GrabSelection::SceneItem(entity),
                        position,
                        entity.to_bits(),
                    ))
                })
                .chain(self.doors.iter().filter_map(|(entity, transform, door)| {
                    can_grab_door_from_inside(
                        actor.translation(),
                        door.doorway_centre_metres,
                        door.tangent,
                        door.outward,
                        door.size_metres.x * 0.5,
                    )
                    .then(|| transform.translation())
                    .filter(|position| self.visible(origin, *position))
                    .map(|position| (GrabSelection::Door(entity), position, entity.to_bits()))
                })),
        )
    }

    fn visible(&self, origin: Vec3, position: Vec3) -> bool {
        let sight = position - origin;
        let distance = sight.length();
        distance > f32::EPSILON
            && Dir3::new(sight / distance).is_ok_and(|direction| {
                self.spatial
                    .cast_ray(
                        origin,
                        direction,
                        distance,
                        true,
                        &SpatialQueryFilter::from_mask(TACTICAL_TERRAIN_LAYER),
                    )
                    .is_none()
            })
    }
}

pub(super) fn world_grab_selection(
    selection: Option<GrabSelection>,
    hand_occupied: bool,
    pointed: Option<GrabSelection>,
) -> Option<GrabSelection> {
    if hand_occupied
        || !matches!(
            selection,
            None | Some(GrabSelection::SceneItem(_) | GrabSelection::Door(_))
        )
    {
        return selection;
    }
    pointed
}

pub(super) fn update_pickup_outlines(
    session: Res<GrabSession>,
    mut outlines: Query<(&GrabTargetOutline, &mut OutlineVolume)>,
) {
    for (outline, mut volume) in &mut outlines {
        volume.visible = grab_target_outline_selected(outline.0, session.selection);
    }
}

pub(super) fn grab_target_outline_selected(
    target: Entity,
    selection: Option<GrabSelection>,
) -> bool {
    matches!(
        selection,
        Some(GrabSelection::SceneItem(selected) | GrabSelection::Door(selected)) if selected == target
    )
}
