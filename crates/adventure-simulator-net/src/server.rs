use std::net::SocketAddr;
use std::time::Duration;

use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use lightyear::netcode::NetcodeServer;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use lightyear::prelude::Identity;
use lightyear::websocket::server::WebSocketServerIo;
use lightyear::webtransport::server::WebTransportServerIo;

use crate::prelude::{ProtocolSettings, ServerProtocol, WebTransportCertificateSettings};
use crate::{DEFAULT_SERVER_ADDR, FIXED_TICK_DURATION};

#[derive(Default)]
pub struct AdventureSimulatorServerPlugin;

impl Plugin for AdventureSimulatorServerPlugin {
    fn build(&self, app: &mut App) {
        let tick_duration = Duration::from_secs_f64(FIXED_TICK_DURATION);
        app.add_plugins(ServerPlugins { tick_duration });
    }
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
            addr: DEFAULT_SERVER_ADDR,
            protocol: ServerProtocol::default(),
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
                addr,
                protocol,
                protocol_settings,
            } = entity_mut.take::<Self>().unwrap();

            // Insert netcode server config (shared across all transports)
            entity_mut.insert((
                NetcodeServer::new(NetcodeConfig {
                    protocol_id: protocol_settings.id,
                    private_key: protocol_settings.private_key.0,
                    ..Default::default()
                }),
                LocalAddr(addr),
            ));

            // Insert transport-specific IO component
            match protocol {
                ServerProtocol::Udp => {
                    entity_mut.insert(ServerUdpIo::default());
                }
                ServerProtocol::WebTransport { certificate } => {
                    let identity = match certificate {
                        WebTransportCertificateSettings::AutoSelfSigned(sans) => {
                            let identity = Identity::self_signed(sans).unwrap();
                            let digest = identity.certificate_chain().as_slice()[0].hash();
                            // Write digest without colons (lightyear expects plain hex)
                            let digest_hex = digest.to_string().replace(':', "");
                            std::fs::create_dir_all("certificates").ok();
                            std::fs::write("certificates/digest.txt", &digest_hex).ok();
                            info!("WebTransport certificate digest: {digest_hex}");
                            identity
                        }
                        WebTransportCertificateSettings::FromFile { cert: _, key: _ } => {
                            // TODO: Implement async PEM file loading
                            unimplemented!("FromFile certificate loading not yet implemented")
                        }
                    };
                    entity_mut.insert(WebTransportServerIo {
                        certificate: identity,
                    });
                }
                ServerProtocol::WebSocket => {
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
                    entity_mut.insert(WebSocketServerIo { config });
                }
            }

            world.trigger(Start { entity });
            Ok(())
        });
    }
}
