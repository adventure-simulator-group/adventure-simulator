//! Environment-facing authority used by equipment actions.

use adventuresim_tactical_core::prelude::SpatialQuery;
use bevy::ecs::system::SystemParam;

#[derive(SystemParam)]
pub(super) struct EquipmentEnvironment<'w, 's> {
    pub(super) spatial: SpatialQuery<'w, 's>,
    pub(super) doors: crate::doors::DoorGrabber<'w, 's>,
}
