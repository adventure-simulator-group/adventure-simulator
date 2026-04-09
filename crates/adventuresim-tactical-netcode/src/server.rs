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
            .add_systems(Update, open_added_servers);
    }
}

#[derive(Component, Debug, Clone)]
#[require(Name = Name::from("Server"))]
pub struct AdventureSimulatorServer {
    pub addr: SocketAddr,
}

impl Default for AdventureSimulatorServer {
    fn default() -> Self {
        Self { addr: DEFAULT_SERVER_ADDR }
    }
}

fn open_added_servers(
    mut commands: Commands,
    servers: Query<(Entity, &AdventureSimulatorServer), Added<AdventureSimulatorServer>>,
) {
    for (entity, server) in &servers {
        commands.entity(entity).insert(AeronetRepliconServer);
        commands
            .entity(entity)
            .queue(WebSocketServer::open(websocket_config(server.addr)));
    }
}

fn websocket_config(addr: SocketAddr) -> ServerConfig {
    ServerConfig::builder()
        .with_bind_address(addr)
        .with_no_encryption()
}
