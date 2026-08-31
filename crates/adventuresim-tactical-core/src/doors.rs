//! Shared tactical door collision policy.

use avian3d::prelude::*;
use bevy::{
    ecs::{entity::EntityHashSet, system::SystemParam},
    prelude::*,
};

pub const DOOR_GRAB_DEPTH_METRES: f32 = 1.6;
pub const DOOR_GRAB_LATERAL_MARGIN_METRES: f32 = 0.6;
/// Collision layer reserved for interactive tactical doors.
pub const TACTICAL_DOOR_LAYER: LayerMask = LayerMask(1 << 6);
const DOOR_INTERIOR_SIDE_EPSILON_METRES: f32 = 0.05;

pub fn can_grab_door_from_inside(
    character_position: Vec3,
    doorway_centre: Vec3,
    tangent: Vec3,
    outward: Vec3,
    half_width_metres: f32,
) -> bool {
    let offset = character_position - doorway_centre;
    let signed_depth = offset.dot(outward);
    let lateral_distance = offset.dot(tangent).abs();
    (-DOOR_GRAB_DEPTH_METRES..=-DOOR_INTERIOR_SIDE_EPSILON_METRES).contains(&signed_depth)
        && lateral_distance <= half_width_metres + DOOR_GRAB_LATERAL_MARGIN_METRES
}

/// Door colliders ignored by one authoritative character while exiting.
#[derive(Component, Debug, Default)]
pub struct DoorPassageExemptions(EntityHashSet);

impl DoorPassageExemptions {
    pub fn contains(&self, door: Entity) -> bool {
        self.0.contains(&door)
    }

    pub fn grant(&mut self, door: Entity) {
        self.0.insert(door);
    }

    pub fn revoke(&mut self, door: Entity) {
        self.0.remove(&door);
    }
}

/// Pair filter that keeps an interior passage exemption local to its character.
#[derive(SystemParam)]
pub struct TacticalCollisionHooks<'w, 's> {
    exemptions: Query<'w, 's, &'static DoorPassageExemptions>,
}

pub(crate) fn tactical_physics_plugins() -> impl PluginGroup {
    PhysicsPlugins::new(FixedPostUpdate).with_collision_hooks::<TacticalCollisionHooks>()
}

impl CollisionHooks for TacticalCollisionHooks<'_, '_> {
    fn filter_pairs(&self, collider1: Entity, collider2: Entity, _commands: &mut Commands) -> bool {
        !self
            .exemptions
            .get(collider1)
            .is_ok_and(|exemptions| exemptions.contains(collider2))
            && !self
                .exemptions
                .get(collider2)
                .is_ok_and(|exemptions| exemptions.contains(collider1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passage_exemptions_are_specific_to_one_door() {
        let mut exemptions = DoorPassageExemptions::default();
        let allowed = Entity::from_raw_u32(1).expect("valid test entity");
        let blocked = Entity::from_raw_u32(2).expect("valid test entity");

        exemptions.grant(allowed);
        assert!(exemptions.contains(allowed));
        assert!(!exemptions.contains(blocked));
        exemptions.revoke(allowed);
        assert!(!exemptions.contains(allowed));
    }

    #[test]
    fn door_grab_requires_the_interior_side_and_doorway_vicinity() {
        let doorway = Vec3::ZERO;
        let tangent = Vec3::X;
        let outward = Vec3::Z;

        assert!(can_grab_door_from_inside(
            Vec3::new(0.0, 0.0, -1.0),
            doorway,
            tangent,
            outward,
            0.5,
        ));
        assert!(!can_grab_door_from_inside(
            Vec3::new(0.0, 0.0, 0.5),
            doorway,
            tangent,
            outward,
            0.5,
        ));
        assert!(!can_grab_door_from_inside(
            Vec3::new(1.2, 0.0, -1.0),
            doorway,
            tangent,
            outward,
            0.5,
        ));
    }
}
