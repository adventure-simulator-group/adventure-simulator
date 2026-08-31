use adventuresim_tactical_core::prelude::*;
use bevy::{ecs::entity::MapEntities, prelude::*};
use serde::{Deserialize, Serialize};

/// Sent by the client whenever the player dodges or rolls defensively.
///
/// This is a simplified version of [`DefenderResponse`] that omits
/// `input_reflex` — the server computes reflex from timestamp delta.
#[derive(Debug, Clone, Copy, PartialEq, Event, Serialize, Deserialize)]
pub enum DefendRequest {
    Dodge { direction: Vec2 },
    Roll,
}

/// Requests enrollment of a strategic character in the tactical mission.
/// The sender remains the authoritative network identity.
#[derive(Debug, Clone, Copy, Event, Serialize, Deserialize)]
pub struct JoinRequest {
    pub character_id: CharacterId,
    pub reconnect_token: Option<ReconnectToken>,
}

/// A server-generated bearer capability for resuming one transient tactical
/// session. It is sent only to its owning connection and rotated on use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReconnectToken(pub [u8; 32]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Event, Serialize, Deserialize)]
pub struct ReconnectCapability {
    pub character_id: CharacterId,
    pub token: ReconnectToken,
}

/// One bounded, ordered scene asset delivered only to an enrolled client.
/// Vista samples intentionally bypass ordinary ECS component replication.
#[derive(Debug, Clone, Event, Serialize, Deserialize)]
pub struct SceneVistaBundle {
    pub scene_digest: String,
    /// Half-width and half-depth of the authoritative playable heightfield.
    /// Presentation-only vista rings clip exactly to this rectangle.
    pub playable_half_extent_metres: Vec2,
    /// Presentation-only shell instances surrounding the tactical area.
    pub distant_buildings: Vec<DistantBuildingPlacement>,
    pub lods: Vec<VistaLod>,
}

/// Canonical tuning selected by the tactical server for this process lifetime.
/// Clients consume this snapshot instead of loading an independent local file.
#[derive(Debug, Clone, Event, Serialize, Deserialize)]
pub struct TacticalCombatConfigSnapshot(pub TacticalCombatConfig);

#[derive(Debug, Clone, Copy, Default, Event, Serialize, Deserialize, Reflect)]
#[reflect(Default)]
pub struct PlayerInputRequest {
    /// Client fixed tick that sampled this complete input state. The server
    /// rejects older samples so reordering on the unreliable channel cannot
    /// restore stale movement or look intent. It is also the reconciliation
    /// acknowledgement key for future client rollback.
    pub simulation_tick: u32,
    pub movement: Option<Vec2>,
    pub look: Vec2,
    pub jump: JumpCommand,
    pub jump_charge: bool,
    pub downed_align: bool,
    pub posture: PostureCommand,
    pub pace: MovementPace,
    pub weapon_guard: WeaponGuardState,
    pub melee_preparation: MeleePreparationInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EquipmentHand {
    Left,
    Right,
}

impl EquipmentHand {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
        }
    }

    pub const fn slot(self) -> EquipSlot {
        match self {
            Self::Left => EquipSlot::HoldingLeft,
            Self::Right => EquipSlot::HoldingRight,
        }
    }

    pub const fn location(self) -> EquipmentLocation {
        match self {
            Self::Left => EquipmentLocation::LeftHand,
            Self::Right => EquipmentLocation::RightHand,
        }
    }
}

/// One ordered, mapped command is committed when a grab button is released.
/// Slot depth is a presentation selection; the server recomputes the ordered
/// candidate list from its authoritative topology before mutating anything.
#[derive(Debug, Clone, Copy, Event, Serialize, Deserialize, MapEntities)]
pub struct EquipmentActionRequest {
    #[entities]
    pub actor: Entity,
    pub sequence: u32,
    pub expected_revision: u32,
    pub hand: EquipmentHand,
    #[entities]
    pub expected_hand_item: Option<Entity>,
    #[entities]
    pub action: EquipmentAction,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, MapEntities)]
pub enum EquipmentAction {
    Slot {
        location: EquipmentLocation,
        depth: u16,
        #[entities]
        expected_destination: Option<Entity>,
    },
    Hand {
        destination: EquipmentHand,
        #[entities]
        expected_destination: Option<Entity>,
    },
    Drop,
    Pickup {
        #[entities]
        item: Entity,
    },
}

/// Durable edge identity for jumping over the unreliable continuous-input
/// channel. The latest sequence is repeated in every input packet, so dropping
/// the release packet delays a jump rather than losing it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
pub struct JumpCommand {
    pub sequence: u32,
    /// Camera-relative quickstep direction selected on this edge. `None`
    /// requests an ordinary jump.
    pub quickstep: Option<Vec2>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub struct PostureCommand {
    pub sequence: u32,
    pub action: Option<PostureActionRequest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum PostureActionRequest {
    Toggle,
    RollLeft,
    RollRight,
    Dive {
        animation_direction: DiveDirection,
        travel_direction: DiveDirection,
    },
}

/// Debug-build request to run the tactical simulation at normal or quarter
/// speed. Production servers intentionally do not install a handler for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Event, Serialize, Deserialize)]
pub struct DebugGameTimeScaleRequest {
    pub quarter_speed: bool,
}

/// Debug-build request to serialize the server's entire world (every entity's
/// reflected components, plus reflected resources) to a `.scn.ron` file for
/// offline inspection or replay as a test fixture. Production servers
/// intentionally do not install a handler for it.
#[derive(Debug, Clone, Copy, Event, Serialize, Deserialize)]
pub struct DebugDumpWorldRequest;

impl DebugGameTimeScaleRequest {
    pub const fn relative_speed(self) -> f32 {
        if self.quarter_speed { 0.25 } else { 1.0 }
    }
}

#[cfg(test)]
mod debug_game_time_scale_tests {
    use super::DebugGameTimeScaleRequest;

    #[test]
    fn request_maps_to_normal_or_quarter_speed() {
        assert_eq!(
            DebugGameTimeScaleRequest {
                quarter_speed: false
            }
            .relative_speed(),
            1.0
        );
        assert_eq!(
            DebugGameTimeScaleRequest {
                quarter_speed: true
            }
            .relative_speed(),
            0.25
        );
    }
}

#[cfg(test)]
mod equipment_action_mapping_tests {
    use super::{EquipmentAction, EquipmentActionRequest, EquipmentHand};
    use adventuresim_tactical_core::prelude::EquipmentLocation;
    use bevy::ecs::entity::{EntityHashMap, MapEntities};
    use bevy::prelude::Entity;

    #[test]
    fn equipment_hand_ids_have_explicit_stable_values() {
        assert_eq!(EquipmentHand::Left.id(), "Left");
        assert_eq!(EquipmentHand::Right.id(), "Right");
    }

    #[test]
    fn request_maps_entities_nested_inside_equipment_action() {
        let actor = Entity::from_bits(1);
        let hand_item = Entity::from_bits(2);
        let destination = Entity::from_bits(3);
        let mapped_actor = Entity::from_bits(11);
        let mapped_hand_item = Entity::from_bits(12);
        let mapped_destination = Entity::from_bits(13);
        let mut mapper = EntityHashMap::default();
        mapper.insert(actor, mapped_actor);
        mapper.insert(hand_item, mapped_hand_item);
        mapper.insert(destination, mapped_destination);

        let mut request = EquipmentActionRequest {
            actor,
            sequence: 1,
            expected_revision: 0,
            hand: EquipmentHand::Left,
            expected_hand_item: Some(hand_item),
            action: EquipmentAction::Slot {
                location: EquipmentLocation::LeftBelt,
                depth: 0,
                expected_destination: Some(destination),
            },
        };
        request.map_entities(&mut mapper);

        assert_eq!(request.actor, mapped_actor);
        assert_eq!(request.expected_hand_item, Some(mapped_hand_item));
        assert!(matches!(
            request.action,
            EquipmentAction::Slot {
                expected_destination: Some(found),
                ..
            } if found == mapped_destination
        ));
    }

    #[test]
    fn request_maps_pickup_target_nested_inside_equipment_action() {
        let item = Entity::from_bits(4);
        let mapped_item = Entity::from_bits(14);
        let mut request = EquipmentActionRequest {
            actor: Entity::from_bits(1),
            sequence: 1,
            expected_revision: 0,
            hand: EquipmentHand::Left,
            expected_hand_item: None,
            action: EquipmentAction::Pickup { item },
        };

        request.map_entities(&mut (item, mapped_item));

        assert!(matches!(
            request.action,
            EquipmentAction::Pickup { item: found } if found == mapped_item
        ));
    }
}

#[derive(Debug, Clone, Copy, Event, Serialize, Deserialize, MapEntities)]
pub struct MeleeActionRequest {
    pub strike_family: StrikeFamily,
    pub hand: AttackHand,
    #[entities]
    pub target: Option<Entity>,
}

/// Both ranged phases share one mapped ordered stream. A completion may omit
/// a target when the client-fired shot missed, but it still consumes ammo.
#[derive(Debug, Clone, Copy, Event, Serialize, Deserialize, MapEntities)]
pub enum RangedActionRequest {
    Start {
        #[entities]
        target: Option<Entity>,
    },
    CompleteMiss,
    CompleteHit {
        #[entities]
        target: Entity,
        body_part: BodyPart,
        reported_precision: f32,
    },
}

#[cfg(test)]
mod combat_action_mapping_tests {
    use super::{MeleeActionRequest, RangedActionRequest};
    use adventuresim_tactical_core::prelude::{AttackHand, StrikeFamily};
    use bevy::ecs::entity::MapEntities;
    use bevy::prelude::Entity;

    #[test]
    fn attack_starts_map_their_acquired_target() {
        let target = Entity::from_bits(21);
        let mapped = Entity::from_bits(22);
        let mut melee = MeleeActionRequest {
            strike_family: StrikeFamily::Swing,
            hand: AttackHand::Main,
            target: Some(target),
        };
        let mut ranged = RangedActionRequest::Start {
            target: Some(target),
        };

        melee.map_entities(&mut (target, mapped));
        ranged.map_entities(&mut (target, mapped));

        assert!(matches!(
            melee,
            MeleeActionRequest {
                target: Some(found),
                ..
            } if found == mapped
        ));
        assert!(matches!(
            ranged,
            RangedActionRequest::Start {
                target: Some(found)
            } if found == mapped
        ));
    }
}

#[derive(Debug, Clone, Event, Serialize, Deserialize, MapEntities)]
pub struct SuccessfulAttackResponse {
    #[entities]
    pub attacker: Entity,
    #[entities]
    pub hit: Vec<Entity>,
    pub body_part: BodyPart,
    pub result: AttackResult,
    pub flanking: f32,
    pub defender_response: DefenderResponse,
    /// Entity whose controller received `impact_velocity_change`.
    #[entities]
    pub impact_recipient: Entity,
    /// Server-applied change in world-space linear velocity, in metres per
    /// second. Presentation consumes the identical value for secondary motion.
    pub impact_velocity_change: Vec3,
    /// Canonical impact position in the hit character's local space.
    pub impact_point: Vec3,
    /// Canonical outward surface normal in the hit character's local space.
    pub impact_normal: Vec3,
    /// Server-authoritative presentation semantics for this contact.
    pub impact_effects: ImpactEffects,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactEffects {
    pub metal_sparks: bool,
    pub blood: bool,
    pub sound: ImpactSound,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImpactSound {
    #[default]
    None,
    Metal,
    CutFlesh,
    BluntFlesh,
    NonMetalWeapon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TacticalOutcome {
    Victory,
    Defeat,
}

/// Broadcast only after strategic authority accepts the terminal tactical
/// submission. It is presentation state, not a second outcome authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Event, Serialize, Deserialize)]
pub struct TacticalOutcomeResponse {
    pub outcome: TacticalOutcome,
}

impl SuccessfulAttackResponse {
    pub fn total_damage(&self) -> f32 {
        match self.result {
            AttackResult::ToAttacker { .. } => 0.0,
            AttackResult::ToDefender {
                cut_damage,
                blunt_damage,
                ..
            } => cut_damage + blunt_damage,
        }
    }
}
