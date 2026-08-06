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
        AttackSpec, BODY_TURN_SPEED_RADIANS, BlockSpec, BodyState, CROUCH_LOCOMOTION_PROFILE,
        DodgeSpec, Footwork, GroundedPosture, HUMANOID_LANDING_PROFILE, LOCOMOTION_SAMPLE_HZ,
        LandingProfile, LeadFoot, LocomotionGait, LocomotionProfile, PackValidationError,
        PoseSample, PoseSampling, Posture, RAISED_GUARD_LOCOMOTION_PROFILE, RUN_LOCOMOTION_PROFILE,
        RaisedLocomotionIntent, ResolvedPose, SemanticPose, SkeletonAction,
        SkeletonLocomotionInput, SkeletonState, StanceState, StrikeFamily, WALK_LOCOMOTION_PROFILE,
        WeaponGuardState, advance_body_facing, controller_yaw, gait_cycle_phase_delta,
        gait_support_weights, guard_step_length, locomotion_profile, ordinary_step_distance,
        project_skeleton_locomotion, set_weapon_guard,
    };
    pub use crate::combat::{Attack, Dodge, HANDS_REACH, Parry, melee_interaction_range};
    pub use crate::inventory::{
        ArmorItem, ArmorSide, ArmorSlot, EquipSlot, EquipmentTopology, EquipmentTopologyOccupancy,
        InventoryItems, ItemOf, ItemProperties, ItemQuantity, ShieldItem, WeaponItem,
    };
    pub use crate::physics::{
        AdventureSimulatorPhysicsSet, TACTICAL_GUARD_SPEED_METRES_PER_SECOND,
        TACTICAL_RUN_SPEED_METRES_PER_SECOND, tactical_character_controller,
        tactical_movement_speed, tactical_movement_speed_for_guard,
    };
    pub use crate::player::{
        Attributes, BestiaryCategories, CharacterId, ControlledPlayer, Limbs, Player, Skills,
        Stats, TacticalCombatState, TacticalPlayerView, TacticalPlayerViewer,
    };
    pub use crate::scene::{SceneId, SceneTerrain, TerrainGenerator};
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
