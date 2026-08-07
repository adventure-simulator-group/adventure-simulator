use adventuresim_tactical_core::prelude::*;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_replicon::prelude::*;

use crate::FIXED_TIMESTEP_HZ;
use crate::message::{
    DebugGameTimeScaleRequest, DefendRequest, JoinRequest, MeleeActionRequest, PlayerInputRequest,
    RangedActionRequest, SuccessfulAttackResponse, TacticalOutcomeResponse,
};

#[derive(Default)]
pub struct AdventureSimulatorReplicationPlugin;

impl Plugin for AdventureSimulatorReplicationPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<StatesPlugin>() {
            app.add_plugins(StatesPlugin);
        }

        app.insert_resource(Time::<Fixed>::from_hz(FIXED_TIMESTEP_HZ as f64))
            .add_plugins(RepliconPlugins)
            .replicate::<Player>()
            .replicate::<CharacterId>()
            .replicate::<Limbs>()
            .replicate::<CombatState>()
            .replicate::<Skills>()
            .replicate::<Stats>()
            .replicate::<TacticalCombatState>()
            .replicate::<SkeletonState>()
            .replicate::<Attributes>()
            .replicate::<Transform>()
            .replicate::<CharacterLook>()
            .replicate::<WeaponItem>()
            .replicate::<ShieldItem>()
            .replicate::<ArmorItem>()
            .replicate::<ItemQuantity>()
            .replicate::<ItemProperties>()
            .replicate::<EquipSlot>()
            .replicate::<ItemOf>()
            .replicate::<SceneId>()
            .replicate::<SceneTerrain>()
            .add_client_event::<JoinRequest>(Channel::Ordered)
            .add_client_event::<PlayerInputRequest>(Channel::Unreliable)
            .add_client_event::<DebugGameTimeScaleRequest>(Channel::Ordered)
            .add_client_event::<DefendRequest>(Channel::Unreliable)
            .add_mapped_client_event::<MeleeActionRequest>(Channel::Ordered)
            .add_mapped_client_event::<RangedActionRequest>(Channel::Ordered)
            .add_mapped_server_event::<SuccessfulAttackResponse>(Channel::Ordered)
            .add_server_event::<TacticalOutcomeResponse>(Channel::Ordered);

        // Replicating physics components since those don't change and
        // it's useful for debugging. Can be gated behind a feature flag, but
        // that's error prone because of how client/server is built independently.
        app.replicate_once_filtered::<Collider, Or<(With<Player>, With<Sensor>)>>()
            .replicate_once_filtered::<RigidBody, With<Player>>();
    }
}
