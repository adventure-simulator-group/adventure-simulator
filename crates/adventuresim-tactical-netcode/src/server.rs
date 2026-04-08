use std::net::SocketAddr;

use aeronet_replicon::server::{AeronetRepliconServer, AeronetRepliconServerPlugin};
use aeronet_websocket::{
    server::{HandshakeHandler, ServerConfig, WebSocketServer, WebSocketServerPlugin},
    tungstenite::handshake::server::ErrorResponse,
};
use bevy::prelude::*;

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
        Self {
            addr: SocketAddr::from(([127, 0, 0, 1], 6000)),
        }
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
        .with_handshake_handler(HandshakeHandler::new(|req, resp| {
            let origin = req
                .headers()
                .get("origin")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("<none>");
            let host = req
                .headers()
                .get("host")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("<none>");
            let user_agent = req
                .headers()
                .get("user-agent")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("<none>");

            info!(
                "WebSocket handshake: uri={} host={} origin={} user-agent={}",
                req.uri(),
                host,
                origin,
                user_agent,
            );

            Ok::<_, ErrorResponse>(resp)
        }))
}
