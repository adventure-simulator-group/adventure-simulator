use adventuresim_tactical_core::prelude::*;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_replicon::{
    bytes::Bytes,
    postcard_utils,
    prelude::*,
    shared::replication::registry::{
        ctx::{SerializeCtx, WriteCtx},
        rule_fns::RuleFns,
    },
};

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
            .replicate::<TacticalAttributes>()
            .replicate::<Transform>()
            .replicate::<CharacterLook>()
            .replicate::<WeaponItem>()
            .replicate::<ShieldItem>()
            .replicate::<ArmorItem>()
            .replicate::<TacticalItemQuantity>()
            .replicate::<ItemProperties>()
            .replicate::<EquipmentTopology>()
            .replicate::<TacticalEquipmentPhysical>()
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
            .replicate_once::<FaultScarpRecipe>()
            .replicate_with(RuleFns::new(
                serialize_scene_building,
                deserialize_scene_building,
            ))
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

        // Replicate compact static physics components for client diagnostics.
        // Building compound colliders remain server-authoritative: the client
        // derives the visual origin from the replicated program, while sending
        // every wall/roof collision cuboid can overflow the transport budget.
        app.replicate_once_filtered::<Collider, Or<(
            With<Player>,
            With<Sensor>,
            With<TacticalSceneItem>,
            With<SceneObstacle>,
            With<SceneId>,
        )>>()
        .replicate_once_filtered::<RigidBody, Or<(
            With<Player>,
            With<TacticalSceneItem>,
            With<SceneObstacle>,
            With<SceneId>,
        )>>()
        .replicate_once_filtered::<CollisionLayers, Or<(
            With<TacticalSceneItem>,
            With<SceneObstacle>,
            With<SceneId>,
        )>>();
    }
}

fn serialize_scene_building(
    _ctx: &mut SerializeCtx,
    building: &SceneBuilding,
    message: &mut Vec<u8>,
) -> Result<()> {
    let json = serde_json::to_vec(building)?;
    postcard_utils::to_extend_mut(&json, message)?;
    Ok(())
}

fn deserialize_scene_building(_ctx: &mut WriteCtx, message: &mut Bytes) -> Result<SceneBuilding> {
    let json: Vec<u8> = postcard_utils::from_buf(message)?;
    Ok(serde_json::from_slice(&json)?)
}

#[cfg(test)]
mod tests {
    use adventuresim_building_generator::{BuildingArchetype, BuildingProgram};

    use super::*;

    fn building() -> SceneBuilding {
        SceneBuilding {
            id: 7,
            program: BuildingProgram::fixture(BuildingArchetype::FachwerkMerchantHouse, 47),
            quarter_turns: 3,
        }
    }

    #[test]
    fn scene_building_json_payload_round_trips_through_postcard() {
        let building = building();
        let json = serde_json::to_vec(&building).unwrap();
        let mut bytes = Vec::new();
        postcard_utils::to_extend_mut(&json, &mut bytes).unwrap();
        let decoded_json: Vec<u8> = bevy_replicon::postcard::from_bytes(&bytes).unwrap();
        let decoded: SceneBuilding = serde_json::from_slice(&decoded_json).unwrap();

        assert_eq!(decoded, building);
    }

    #[test]
    fn default_postcard_cannot_decode_internally_tagged_building_programs() {
        let building = building();
        let mut bytes = Vec::new();
        postcard_utils::to_extend_mut(&building, &mut bytes).unwrap();

        assert!(bevy_replicon::postcard::from_bytes::<SceneBuilding>(&bytes).is_err());
    }
}
