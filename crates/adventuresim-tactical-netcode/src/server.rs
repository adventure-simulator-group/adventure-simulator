use std::net::SocketAddr;

use aeronet_replicon::server::{AeronetRepliconServer, AeronetRepliconServerPlugin};
use aeronet_websocket::server::{ServerConfig, WebSocketServer, WebSocketServerPlugin};
use bevy::prelude::*;

use crate::DEFAULT_SERVER_ADDR;

#[derive(Default)]
pub struct AdventureSimulatorServerPlugin;

impl Plugin for AdventureSimulatorServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((WebSocketServerPlugin, AeronetRepliconServerPlugin))
            .add_observer(on_server_added);
    }
}

#[derive(Component, Debug, Clone)]
#[require(Name::from("Server"), AeronetRepliconServer)]
pub struct AdventureSimulatorServer {
    pub addr: SocketAddr,
}

impl Default for AdventureSimulatorServer {
    fn default() -> Self {
        Self {
            addr: DEFAULT_SERVER_ADDR,
        }
    }
}

fn on_server_added(
    event: On<Add, AdventureSimulatorServer>,
    mut commands: Commands,
    servers: Query<&AdventureSimulatorServer>,
) -> Result {
    let server = servers.get(event.entity)?;

    commands
        .entity(event.entity)
        .queue(WebSocketServer::open(websocket_config(server.addr)));

    Ok(())
}

fn websocket_config(addr: SocketAddr) -> ServerConfig {
    ServerConfig::builder()
        .with_bind_address(addr)
        .with_no_encryption()
}
