use std::net::SocketAddr;
use std::time::Duration;

use bevy::ecs::lifecycle::HookContext;
use bevy::prelude::*;
use lightyear::netcode::{ConnectToken, NetcodeServer};
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use serde::Deserialize;

use crate::token::{HexConnectToken, HexPrivateKey};
use crate::{DEFAULT_SERVER_ADDR, FIXED_TICK_DURATION};
use bevy::ecs::world::DeferredWorld;

#[derive(Default)]
pub struct AdventureSimulatorServerPlugin;

impl Plugin for AdventureSimulatorServerPlugin {
    fn build(&self, app: &mut App) {
        let tick_duration = Duration::from_secs_f64(FIXED_TICK_DURATION);
        app.add_plugins(ServerPlugins { tick_duration });
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ProtocolSettings {
    /// An id to identify the protocol version
    pub id: u64,
    /// a 32-byte array to authenticate via the Netcode.io protocol
    pub private_key: HexPrivateKey,
}

#[derive(Component, Debug, Clone)]
#[component(immutable, on_add = Self::on_add)]
#[require(Name = Name::from("Server"), Server)]
pub struct AdventureSimulatorServer {
    pub addr: SocketAddr,
    pub protocol: ServerProtocol,
    pub protocol_settings: ProtocolSettings,
}

impl Default for AdventureSimulatorServer {
    fn default() -> Self {
        Self {
            protocol: ServerProtocol::WebSocket,
            addr: DEFAULT_SERVER_ADDR,
            protocol_settings: default(),
        }
    }
}

impl AdventureSimulatorServer {
    fn on_add(mut world: DeferredWorld, context: HookContext) {
        let entity = context.entity;
        world.commands().queue(move |world: &mut World| -> Result {
            let mut entity_mut = world.entity_mut(entity);
            let Self {
                protocol,
                protocol_settings,
                addr,
            } = entity_mut.take::<Self>().unwrap();

            let sans = vec![
                "localhost".to_string(),
                "127.0.0.1".to_string(),
                "::1".to_string(),
            ];
            let config = ServerConfig::builder()
                .with_bind_address(addr)
                .with_identity(lightyear::websocket::server::Identity::self_signed(sans).unwrap());

            entity_mut.insert((
                NetcodeServer::new(NetcodeConfig {
                    private_key: *protocol_settings.private_key,
                    protocol_id: protocol_settings.id,
                    ..default()
                }),
                LocalAddr(addr),
                WebSocketServerIo { config },
            ));

            world.trigger(Start { entity });
            Ok(())
        });
    }

    pub fn generate_token(&self, client_id: u64) -> Result<HexConnectToken> {
        let token = ConnectToken::build(
            self.addr,
            self.protocol_settings.id,
            client_id,
            *self.protocol_settings.private_key,
        )
        .expire_seconds(60)
        .timeout_seconds(5)
        .generate()?;

        Ok(HexConnectToken::new(token.try_into_bytes()?))
    }
}
