use std::net::SocketAddr;
use std::time::Duration;

use bevy::ecs::lifecycle::HookContext;
use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
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

impl From<&WebTransportCertificateSettings> for Identity {
    fn from(wt: &WebTransportCertificateSettings) -> Identity {
        match wt {
            WebTransportCertificateSettings::AutoSelfSigned(sans) => {
                // In addition to and Subject Alternate Names (SAN) added via the config,
                // we add the public ip and domain for edgegap, if detected, and also
                // any extra values specified via the SELF_SIGNED_SANS environment variable.
                let mut sans = sans.clone();
                // Are we running on edgegap?
                // TODO: remove `std::env::var`
                if let Ok(public_ip) = std::env::var("ARBITRIUM_PUBLIC_IP") {
                    info!("🔐 SAN += ARBITRIUM_PUBLIC_IP: {public_ip}");
                    sans.push(public_ip);
                    sans.push("*.pr.edgegap.net".to_string());
                }
                // generic env to add domains and ips to SAN list:
                // SELF_SIGNED_SANS="example.org,example.com,127.1.1.1"
                // TODO: remove `std::env::var`
                if let Ok(san) = std::env::var("SELF_SIGNED_SANS") {
                    info!("🔐 SAN += SELF_SIGNED_SANS: {san}");
                    sans.extend(san.split(',').map(|s| s.to_string()));
                }
                info!("🔐 Generating self-signed certificate with SANs: {sans:?}");
                let identity = Identity::self_signed(sans).unwrap();
                let digest = identity.certificate_chain().as_slice()[0].hash();
                info!("🔐 Certificate digest: {digest}");
                identity
            }
            WebTransportCertificateSettings::FromFile {
                cert: cert_pem_path,
                key: private_key_pem_path,
            } => {
                info!(
                    "Reading certificate PEM files:\n * cert: {cert_pem_path}\n * key: {private_key_pem_path}",
                );
                // this is async because we need to load the certificate from io
                // we need async_compat because wtransport expects a tokio reactor
                let identity = IoTaskPool::get()
                    .scope(|s| {
                        s.spawn(async_compat::Compat::new(async {
                            Identity::load_pemfiles(cert_pem_path, private_key_pem_path)
                                .await
                                .unwrap()
                        }));
                    })
                    .pop()
                    .unwrap();
                let digest = identity.certificate_chain().as_slice()[0].hash();
                info!("🔐 Certificate digest: {digest}");
                identity
            }
        }
    }
}
