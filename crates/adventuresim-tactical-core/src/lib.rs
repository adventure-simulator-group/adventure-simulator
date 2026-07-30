#![feature(iter_array_chunks)]

//! Core adventuresim bevy-centric library that is used both by
//! tactical client and tactical server.
//!
//! It defines how the tactical world works in minimal environemnt,
//! which can be extended by networking and visuals in other crates.

pub mod combat;
pub mod inventory;
pub mod physics;
pub mod player;
pub mod scene;

pub use avian3d;

pub mod prelude {
    pub use crate::AdventureSimulatorCorePlugins;
    pub use crate::combat::{Attack, Dodge, Parry};
    pub use crate::inventory::{
        ArmorItem, ArmorSide, ArmorSlot, EquipSlot, InventoryItems, ItemOf, ItemProperties,
        ItemQuantity, ShieldItem, WeaponItem,
    };
    pub use crate::player::{
        Attributes, BestiaryCategories, ControlledPlayer, Limbs, Player, PlayerId, Skills, Stats,
        TacticalPlayerView, TacticalPlayerViewer,
    };
    pub use crate::scene::{SceneId, SceneTerrain};
    pub use adventuresim_core::prelude::*;
    pub use avian3d::prelude::*;
    pub use bevy_ahoy::{
        CharacterController, CharacterControllerState, CharacterLook,
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
