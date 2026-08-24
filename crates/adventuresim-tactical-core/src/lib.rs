//! Core adventuresim bevy-centric library that is used both by
//! tactical client and tactical server.
//!
//! It defines how the tactical world works in minimal environemnt,
//! which can be extended by networking and visuals in other crates.

pub mod animation;
pub mod combat;
pub mod combat_config;
pub mod inventory;
pub mod physics;
pub mod player;
pub mod scene;
pub mod scene_input;

pub use avian3d;

pub mod prelude {
    pub use crate::AdventureSimulatorCorePlugins;
    pub use crate::animation::{
        ActionState, ActionTransitionError, AnimationEvaluation, AnimationPack,
        AnimationPackLibrary, AttackAnimation, AttackAnimations, AttackCurve, AttackHand,
        AttackLine, AttackPreparation, AttackSpec, BODY_TURN_SPEED_RADIANS, BlockSpec, BodyState,
        DOWNED_TURN_SPEED_RADIANS, DiveDirection, DodgeSpec, DownedFacingPose,
        GUARD_CONTACT_MARGIN_METRES, GUARD_MAXIMUM_UNSUPPORTED_CONTACT_SECONDS, GroundedPosture,
        GuardContacts, GuardFootworkPlan, GuardStepPlan, HUMANOID_LANDING_PROFILE,
        JumpAnticipation, LOCOMOTION_SAMPLE_HZ, LandingProfile, LeadFoot, LocomotionGait,
        LocomotionProfile, MeleePreparationInput, PackValidationError, PoseSample, PoseSampling,
        Posture, PostureTransitionKind, PostureTransitionState, RAISED_GUARD_LOCOMOTION_PROFILE,
        RUN_LOCOMOTION_PROFILE, RaisedLocomotionIntent, ResolvedPose, RollDirection, SemanticPose,
        SkeletonAction, SkeletonLocomotionInput, SkeletonState, StanceState, StrikeFamily,
        WALK_LOCOMOTION_PROFILE, WeaponGuardState, advance_body_facing,
        advance_body_facing_with_speed, advance_downed_body_facing,
        advance_downed_body_facing_with_speed, controller_yaw, dive_landing_facing_delta,
        downed_camera_roll_target, gait_cycle_phase_delta, gait_support_weights,
        guard_closed_foot_separation, guard_contact_travel_distance, guard_maximum_foot_separation,
        guard_maximum_lateral_foot_separation, guard_movement_front_foot,
        guard_open_foot_separation, guard_rear_contact_separation, guard_step_length,
        locomotion_profile, ordinary_step_distance, project_skeleton_locomotion,
        project_skeleton_locomotion_with_intent, set_weapon_guard, supine_get_up_counter_yaw_delta,
    };
    pub use crate::combat::{Attack, Dodge, HANDS_REACH, Parry, melee_interaction_range};
    pub use crate::combat_config::*;
    pub use crate::inventory::{
        ArmorItem, ArmorSide, ArmorSlot, EquipSlot, EquipmentActionState, EquipmentPhysical,
        EquipmentTopology, EquipmentTopologyOccupancy, InventoryItems, ItemOf, ItemProperties,
        ItemQuantity, ShieldItem, TACTICAL_ITEM_LAYER, TACTICAL_TERRAIN_LAYER,
        TacticalEquipmentAnchor, TacticalSceneItem, WeaponAppearance, WeaponHolderAppearance,
        WeaponItem, rebuild_inventory_holding_cache,
    };
    pub use crate::physics::{
        AdventureSimulatorPhysicsSet, BREATH_PER_METRE_PER_SECOND, CharacterMotionSnapshot,
        MovementPace, QuickstepPush, TACTICAL_BREATH_RESPONSE_SCALE,
        TACTICAL_GUARD_SPEED_METRES_PER_SECOND, TACTICAL_PRONE_LATERAL_SPEED_SCALE,
        TACTICAL_PRONE_SPEED_METRES_PER_SECOND, TACTICAL_PRONE_WALK_SPEED_METRES_PER_SECOND,
        TACTICAL_ROLL_SPEED_METRES_PER_SECOND, TACTICAL_RUN_SPEED_METRES_PER_SECOND,
        TACTICAL_WALK_SPEED_METRES_PER_SECOND, quickstep_action_contact_ticks,
        quickstep_force_curve, quickstep_peak_horizontal_force_newtons, quickstep_push_seconds,
        tactical_breath_recovery_per_second, tactical_character_controller,
        tactical_exhaustion_change_per_second, tactical_jog_speed,
        tactical_movement_exhaustion_change_per_second, tactical_movement_speed,
        tactical_movement_speed_for_guard, tactical_movement_speed_for_pace, tactical_sprint_speed,
    };
    pub use crate::player::{
        Attributes, BestiaryCategories, CharacterId, ControlledPlayer, Limbs, Player, Skills,
        Stats, TacticalCombatSide, TacticalCombatState, TacticalIncapacitationSources,
        TacticalPlayerView, TacticalPlayerViewer, attack_preparation_secs, attack_recovery_secs,
        configure_attack_curve, default_tactical_character_id, effective_weapon_handling_skill,
    };
    pub use crate::scene::{
        GroundCover, GroundSubstrate, GroundSurface, SceneGround, SceneId, SceneTerrain,
        TerrainGenerator,
    };
    pub use crate::scene_input::{
        EnvironmentalSample, GeneratedObstacle, GeneratedTacticalScene, ROCK_RADIUS_METRES,
        RockArchetype, RockLithology, RockRecipe, SceneEnvironment, SceneInputError, SceneObstacle,
        SceneRepairReport, SceneSource, TACTICAL_SCENE_GENERATION_VERSION,
        TACTICAL_SCENE_SCHEMA_VERSION, TREE_CANOPY_GROUND_RADIUS_METRES, TREE_TRUNK_HEIGHT_METRES,
        TREE_TRUNK_RADIUS_METRES, TacticalSceneInput, TacticalSurface, TerrainSampleGrid, VistaLod,
        VistaSample,
    };
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
