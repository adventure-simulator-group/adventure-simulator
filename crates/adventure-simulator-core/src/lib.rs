//! Core adventuresim bevy-centric library that is used both by
//! tactical client and tactical server.
//!
//! It defines how the tactical world works in minimal environemnt,
//! which can be extended by networking and visuals in other crates.

pub mod physics;
pub mod player;
pub mod scene;

pub use avian3d;

pub mod prelude {
    pub use crate::player::{Player, PlayerId};
    pub use crate::scene::GameSceneId;
    pub use crate::AdventureSimulatorCorePlugins;
    pub use avian3d::prelude::*;
    pub use bevy_ahoy::{
        camera::CharacterControllerCameraOf, input, CharacterController, CharacterControllerState,
    };
    pub use bevy_enhanced_input::{self, prelude::*};
}

bevy::app::plugin_group! {
    #[derive(Debug)]
    pub struct AdventureSimulatorCorePlugins {
        crate::physics:::AdventureSimulatorPhysicsPlugin,
    }
}
