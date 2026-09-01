//! Equipment request state cleanup across player entity lifecycle changes.

use bevy::prelude::Entity;

use super::{LastEquipmentSequence, PendingEquipmentActions};

pub(crate) fn reconnect_equipment_lifecycle(
    old: Entity,
    new: Entity,
    pending: &mut PendingEquipmentActions,
    sequences: &mut LastEquipmentSequence,
) {
    pending
        .0
        .retain(|(actor, _)| *actor != old && *actor != new);
    sequences.0.remove(&new);
    if let Some(sequence) = sequences.0.remove(&old) {
        sequences.0.insert(new, sequence);
    }
}

pub(crate) fn purge_equipment_lifecycle(
    actor: Entity,
    pending: &mut PendingEquipmentActions,
    sequences: &mut LastEquipmentSequence,
) {
    pending.0.retain(|(queued, _)| *queued != actor);
    sequences.0.remove(&actor);
}
