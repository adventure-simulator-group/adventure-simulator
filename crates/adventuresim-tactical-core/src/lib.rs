#![feature(iter_array_chunks)]

//! Core adventuresim bevy-centric library that is used both by
//! tactical client and tactical server.
//!
//! It defines how the tactical world works in minimal environemnt,
//! which can be extended by networking and visuals in other crates.

pub mod animation;
pub mod combat;
pub mod inventory;
pub mod physics;
pub mod player;
pub mod scene;

pub use avian3d;

pub mod prelude {
    pub use crate::AdventureSimulatorCorePlugins;
    pub use crate::animation::{
        ActionState, AnimationEvaluation, AnimationPack, AnimationPackLibrary, AttackLine,
        AttackSpec, AttackStep, BODY_TURN_SPEED_RADIANS, BlockSpec, BodyState,
        CROUCH_LOCOMOTION_PROFILE, DOWNED_TURN_SPEED_RADIANS, DiveDirection, DodgeSpec,
        DownedFacingPose, Footwork, GroundedPosture, HUMANOID_LANDING_PROFILE, JumpAnticipation,
        LOCOMOTION_SAMPLE_HZ, LandingProfile, LeadFoot, LocomotionGait, LocomotionProfile,
        PackValidationError, PoseSample, PoseSampling, Posture, PostureTransitionKind,
        PostureTransitionState, RAISED_GUARD_LOCOMOTION_PROFILE, RUN_LOCOMOTION_PROFILE,
        RaisedLocomotionIntent, ResolvedPose, RollDirection, SemanticPose, SkeletonAction,
        SkeletonLocomotionInput, SkeletonState, StanceState, StrikeFamily, WALK_LOCOMOTION_PROFILE,
        WeaponGuardState, advance_body_facing, advance_downed_body_facing, controller_yaw,
        dive_landing_facing_delta, downed_camera_roll_target, gait_cycle_phase_delta,
        gait_support_weights, guard_step_length, locomotion_profile, ordinary_step_distance,
        project_skeleton_locomotion, set_weapon_guard, supine_get_up_counter_yaw_delta,
    };
    pub use crate::combat::{Attack, Dodge, HANDS_REACH, Parry, melee_interaction_range};
    pub use crate::inventory::{
        ArmorItem, ArmorSide, ArmorSlot, EquipSlot, EquipmentActionState, EquipmentPhysical,
        EquipmentTopology, EquipmentTopologyOccupancy, InventoryItems, ItemOf, ItemProperties,
        ItemQuantity, ShieldItem, TACTICAL_ITEM_LAYER, TACTICAL_TERRAIN_LAYER,
        TacticalEquipmentAnchor, TacticalSceneItem, WeaponItem, rebuild_inventory_holding_cache,
    };
    pub use crate::physics::{
        AdventureSimulatorPhysicsSet, BREATH_PER_METRE_PER_SECOND, MovementPace,
        TACTICAL_BREATH_RESPONSE_SCALE, TACTICAL_GUARD_SPEED_METRES_PER_SECOND,
        TACTICAL_PRONE_SPEED_METRES_PER_SECOND, TACTICAL_ROLL_SPEED_METRES_PER_SECOND,
        TACTICAL_RUN_SPEED_METRES_PER_SECOND, TACTICAL_SUPINE_SPEED_METRES_PER_SECOND,
        TACTICAL_WALK_SPEED_METRES_PER_SECOND, tactical_breath_recovery_per_second,
        tactical_character_controller, tactical_exhaustion_change_per_second, tactical_jog_speed,
        tactical_movement_acceleration_hz_for_guard,
        tactical_movement_exhaustion_change_per_second, tactical_movement_speed,
        tactical_movement_speed_for_guard, tactical_movement_speed_for_pace, tactical_sprint_speed,
    };
    pub use crate::player::{
        Attributes, BestiaryCategories, CharacterId, ControlledPlayer, Limbs, Player, Skills,
        Stats, TacticalCombatState, TacticalIncapacitationSources, TacticalPlayerView,
        TacticalPlayerViewer, default_tactical_character_id,
    };
    pub use crate::scene::{SceneId, SceneTerrain, TerrainGenerator};
    pub use adventuresim_core::item_catalog;
    pub use adventuresim_core::item_catalog::{EquipmentChannel, EquipmentLocation};
    pub use adventuresim_core::prelude::*;
    pub use avian3d::prelude::*;
    pub use bevy_ahoy::{
        AhoySystems, CharacterController, CharacterControllerState, CharacterLook,
        camera::{CharacterControllerCamera, CharacterControllerCameraOf},
        input,
    };
    pub use bevy_enhanced_input::{self, prelude::*};
}

bevy::app::plugin_group! {
    #[derive(Debug)]
    pub struct AdventureSimulatorCorePlugins {
        physics:::AdventureSimulatorPhysicsPlugin,
    }
}
