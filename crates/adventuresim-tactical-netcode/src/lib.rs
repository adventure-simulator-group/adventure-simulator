#[cfg(all(target_arch = "wasm32", feature = "server"))]
compile_error!("The `server` feature cannot be enabled when compiling for wasm32.");

#[cfg(feature = "client")]
pub mod client;
pub mod message;
pub mod replication;
#[cfg(feature = "server")]
pub mod server;

pub use aeronet;
pub use aeronet_replicon;
pub use aeronet_websocket;
pub use bevy_replicon;

pub mod prelude {
    pub use crate::AdventureSimulatorNetPlugins;
    #[cfg(feature = "client")]
    pub use crate::client::{
        AdventureSimulatorClient, DebugForceAttackTrigger, DebugForceQuickstepTrigger,
        PlayerInputOverride,
    };
    pub use crate::message::{
        DebugDumpWorldRequest, DebugGameTimeScaleRequest, DefendRequest, EquipmentAction,
        EquipmentActionRequest, EquipmentHand, JoinRequest, JumpCommand, MeleeActionRequest,
        PlayerInputRequest, PostureActionRequest, PostureCommand, RangedActionRequest,
        ReconnectCapability, ReconnectToken, SceneVistaBundle, TacticalOutcome,
        TacticalOutcomeResponse,
    };
    #[cfg(feature = "server")]
    pub use crate::server::AdventureSimulatorServer;
}

const FIXED_TIMESTEP_HZ: f64 = 64.0;
pub const DEFAULT_SERVER_ADDR: std::net::SocketAddr =
    std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 6000);
pub const DEFAULT_SERVER_URL: &str = "ws://127.0.0.1:6000";

bevy::app::plugin_group! {
    #[derive(Debug)]
    pub struct AdventureSimulatorNetPlugins {
        #[custom(cfg(feature = "server"))]
        crate::server:::AdventureSimulatorServerPlugin,
        #[custom(cfg(feature = "client"))]
        crate::client:::AdventureSimulatorClientPlugin,
        crate::replication:::AdventureSimulatorReplicationPlugin,
    }
}
