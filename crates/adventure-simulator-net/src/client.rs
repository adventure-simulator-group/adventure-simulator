use std::net::SocketAddr;
use std::time::Duration;

use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use lightyear::netcode::client_plugin::NetcodeConfig;
use lightyear::netcode::NetcodeClient;
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use lightyear::websocket::client::WebSocketClientIo;
use lightyear::webtransport::client::WebTransportClientIo;

use crate::prelude::{ClientProtocol, ProtocolSettings};
use crate::{DEFAULT_CLIENT_ADDR, DEFAULT_SERVER_ADDR, FIXED_TICK_DURATION};

#[derive(Default)]
pub struct AdventureSimulatorClientPlugin;

impl Plugin for AdventureSimulatorClientPlugin {
    fn build(&self, app: &mut App) {
        let tick_duration = Duration::from_secs_f64(FIXED_TICK_DURATION);
        app.add_plugins(ClientPlugins { tick_duration });
    }
}

#[derive(Component, Clone, Debug)]
#[component(immutable, on_add = Self::on_add)]
#[require(Name = Name::from("Client"), Client)]
pub struct AdventureSimulatorClient {
    pub id: u64,
    pub server_addr: SocketAddr,
    pub addr: SocketAddr,
    pub protocol: ClientProtocol,
    pub protocol_settings: ProtocolSettings,
}

impl Default for AdventureSimulatorClient {
    fn default() -> Self {
        Self {
            id: 0,
            server_addr: DEFAULT_SERVER_ADDR,
            addr: DEFAULT_CLIENT_ADDR,
            protocol: ClientProtocol::default(),
            protocol_settings: Default::default(),
        }
    }
}

impl AdventureSimulatorClient {
    fn on_add(mut world: DeferredWorld, context: HookContext) {
        let entity = context.entity;
        world.commands().queue(move |world: &mut World| -> Result {
            let mut entity_mut = world.entity_mut(entity);
            let AdventureSimulatorClient {
                id: client_id,
                server_addr,
                addr,
                protocol,
                protocol_settings,
            } = entity_mut.take::<Self>().unwrap();

            // Set up netcode authentication
            let auth = Authentication::Manual {
                server_addr,
                client_id,
                private_key: protocol_settings.private_key.0,
                protocol_id: protocol_settings.id,
            };
            let netcode_config = NetcodeConfig {
                // Make sure that the server times out clients when their connection is closed
                client_timeout_secs: 3,
                token_expire_secs: -1,
                ..default()
            };

            // Insert shared components
            entity_mut.insert((
                NetcodeClient::new(auth, netcode_config)?,
                LocalAddr(addr),
                PeerAddr(server_addr),
                ReplicationReceiver::default(),
            ));

            // Insert transport-specific IO component
            match protocol {
                #[cfg(not(target_family = "wasm"))]
                ClientProtocol::Udp => {
                    entity_mut.insert(UdpIo::default());
                }
                #[cfg(target_family = "wasm")]
                ClientProtocol::Udp => {
                    panic!("UDP is not supported on WASM. Use WebTransport or WebSocket.");
                }
                ClientProtocol::WebTransport { certificate_digest } => {
                    entity_mut.insert(WebTransportClientIo { certificate_digest });
                }
                ClientProtocol::WebSocket => {
                    #[cfg(target_family = "wasm")]
                    let config = ClientConfig::default();
                    #[cfg(not(target_family = "wasm"))]
                    let config = ClientConfig::builder().with_no_cert_validation();
                    entity_mut.insert(WebSocketClientIo { config });
                }
            }

            world.trigger(Connect { entity });
            Ok(())
        });
    }
}

