use adventuresim_tactical_core::prelude::*;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_replicon::prelude::*;

use crate::FIXED_TIMESTEP_HZ;
use crate::message::{
    DebugDumpWorldRequest, DebugGameTimeScaleRequest, DefendRequest, EquipmentActionRequest,
    JoinRequest, MeleeActionRequest, PlayerInputRequest, RangedActionRequest, ReconnectCapability,
    SceneVistaBundle, SuccessfulAttackResponse, TacticalCombatConfigSnapshot,
    TacticalOutcomeResponse,
};

#[derive(Default)]
pub struct AdventureSimulatorReplicationPlugin;

impl Plugin for AdventureSimulatorReplicationPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<StatesPlugin>() {
            app.add_plugins(StatesPlugin);
        }

        app.insert_resource(Time::<Fixed>::from_hz(FIXED_TIMESTEP_HZ))
            .add_plugins(RepliconPlugins)
            .replicate::<Player>()
            .replicate::<CharacterId>()
            .replicate::<CharacterDimensions>()
            .replicate::<Limbs>()
            .replicate::<Skills>()
            .replicate::<Stats>()
            .replicate::<TacticalCombatSide>()
            .replicate::<TacticalCombatState>()
            .replicate::<SkeletonState>()
            .replicate::<CharacterMotionSnapshot>()
            .replicate::<Attributes>()
            .replicate::<Transform>()
            .replicate::<CharacterLook>()
            .replicate::<WeaponItem>()
            .replicate::<ShieldItem>()
            .replicate::<ArmorItem>()
            .replicate::<ItemQuantity>()
            .replicate::<ItemProperties>()
            .replicate::<EquipmentTopology>()
            .replicate::<EquipmentPhysical>()
            .replicate_once::<WeaponAppearance>()
            .replicate_once::<WeaponHolderAppearance>()
            .replicate::<EquipmentActionState>()
            .replicate::<TacticalSceneItem>()
            .replicate::<EquipSlot>()
            .replicate::<ItemOf>()
            .replicate::<SceneId>()
            .replicate::<SceneTerrain>()
            .replicate::<SceneGround>()
            .replicate::<SceneEnvironment>()
            .replicate::<SceneObstacle>()
            .add_client_event::<JoinRequest>(Channel::Ordered)
            .add_server_event::<ReconnectCapability>(Channel::Ordered)
            .add_server_event::<SceneVistaBundle>(Channel::Ordered)
            .add_server_event::<TacticalCombatConfigSnapshot>(Channel::Ordered)
            .add_client_event::<PlayerInputRequest>(Channel::Unreliable)
            .add_client_event::<DebugGameTimeScaleRequest>(Channel::Ordered)
            .add_client_event::<DebugDumpWorldRequest>(Channel::Ordered)
            .add_client_event::<DefendRequest>(Channel::Ordered)
            .add_mapped_client_event::<EquipmentActionRequest>(Channel::Ordered)
            .add_mapped_client_event::<MeleeActionRequest>(Channel::Ordered)
            .add_mapped_client_event::<RangedActionRequest>(Channel::Ordered)
            .add_mapped_server_event::<SuccessfulAttackResponse>(Channel::Ordered)
            .add_server_event::<TacticalOutcomeResponse>(Channel::Ordered);

        // Replicating physics components since those don't change and
        // it's useful for debugging. Can be gated behind a feature flag, but
        // that's error prone because of how client/server is built independently.
        app.replicate_once_filtered::<
            Collider,
            Or<(
                With<Player>,
                With<Sensor>,
                With<TacticalSceneItem>,
                With<SceneObstacle>,
                With<SceneId>,
            )>,
        >()
        .replicate_once_filtered::<
            RigidBody,
            Or<(
                With<Player>,
                With<TacticalSceneItem>,
                With<SceneObstacle>,
                With<SceneId>,
            )>,
        >()
        .replicate_once_filtered::<
            CollisionLayers,
            Or<(With<TacticalSceneItem>, With<SceneObstacle>, With<SceneId>)>,
        >();
    }
}
