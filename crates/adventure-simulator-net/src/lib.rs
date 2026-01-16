#[cfg(all(target_arch = "wasm32", feature = "server"))]
compile_error!("The `server` feature cannot be enabled when compiling for wasm32.");

#[cfg(feature = "client")]
pub mod client;
pub mod netcode;
#[cfg(feature = "server")]
pub mod server;
pub mod token;

pub use lightyear;

pub mod prelude {
    #[cfg(feature = "client")]
    pub use crate::client::AdventureSimulatorClient;
    pub use crate::lightyear;
    #[cfg(feature = "server")]
    pub use crate::server::AdventureSimulatorServer;
    pub use crate::token::{HexConnectToken, HexPrivateKey, HexTokenParsingError};
    pub use crate::AdventureSimulatorNetPlugins;
}

const FIXED_TIMESTEP_HZ: f64 = 64.0;
const FIXED_TICK_DURATION: f64 = 1.0 / FIXED_TIMESTEP_HZ;

/// 0 means that the OS will assign any available port
pub const DEFAULT_CLIENT_PORT: u16 = 0;
pub const DEFAULT_CLIENT_ADDR: std::net::SocketAddr = std::net::SocketAddr::new(
    std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
    DEFAULT_CLIENT_PORT,
);
pub const DEFAULT_SERVER_PORT: u16 = 5888;
pub const DEFAULT_SERVER_ADDR: std::net::SocketAddr = std::net::SocketAddr::new(
    std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
    DEFAULT_SERVER_PORT,
);

bevy::app::plugin_group! {
    #[derive(Debug)]
    pub struct AdventureSimulatorNetPlugins {
        crate::netcode:::AdventureSimulatorNetcodePlugin,
        #[custom(cfg(feature = "server"))]
        crate::server:::AdventureSimulatorServerPlugin,
        #[custom(cfg(feature = "client"))]
        crate::client:::AdventureSimulatorClientPlugin,
    }
}
