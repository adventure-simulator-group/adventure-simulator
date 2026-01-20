use std::net::SocketAddr;
use std::time::Duration;

use bevy::ecs::lifecycle::HookContext;
use bevy::prelude::*;
use lightyear::netcode::NetcodeServer;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use serde::Deserialize;

use crate::prelude::ProtocolSettings;
use crate::protocol::WebTransportCertificateSettings;
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub enum ServerProtocol {
    WebTransport {
        certificate: WebTransportCertificateSettings,
    },
    WebSocket,
}

#[derive(Component, Debug, Clone)]
#[component(immutable, on_add = Self::on_add)]
#[require(Name = Name::from("Server"))]
pub struct AdventureSimulatorServer {
    pub addr: SocketAddr,
    pub protocol: ServerProtocol,
    pub protocol_settings: ProtocolSettings,
}

impl Default for AdventureSimulatorServer {
    fn default() -> Self {
        Self {
            protocol: ServerProtocol::WebTransport {
                certificate: default(),
            },
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

            let add_netcode = |entity_mut: &mut EntityWorldMut| {
                entity_mut.insert(NetcodeServer::new(NetcodeConfig {
                    protocol_id: protocol_settings.id,
                    private_key: protocol_settings.private_key.0,
                    ..Default::default()
                }));
            };
            match protocol {
                ServerProtocol::WebTransport { certificate } => {
                    add_netcode(&mut entity_mut);
                    entity_mut.insert((
                        LocalAddr(addr),
                        WebTransportServerIo {
                            certificate: (&certificate).into(),
                        },
                    ));
                }
                ServerProtocol::WebSocket => {
                    add_netcode(&mut entity_mut);
                    let sans = vec![
                        "localhost".to_string(),
                        "127.0.0.1".to_string(),
                        "::1".to_string(),
                    ];
                    let config = ServerConfig::builder()
                        .with_bind_address(addr)
                        .with_identity(
                            lightyear::websocket::server::Identity::self_signed(sans).unwrap(),
                        );
                    entity_mut.insert((LocalAddr(addr), WebSocketServerIo { config }));
                }
            };
            world.trigger(Start { entity });
            Ok(())
        });
    }
}
