use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::ecs::lifecycle::HookContext;
use bevy::prelude::*;
use lightyear::netcode::{ConnectToken, NetcodeServer};
use lightyear::prelude::rustls::pki_types::pem::PemObject;
use lightyear::prelude::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use lightyear::prelude::server::*;
use lightyear::{prelude::*, websocket};

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
    pub cert: PathBuf,
    pub key: PathBuf,
}

impl Default for AdventureSimulatorServer {
    fn default() -> Self {
        Self {
            protocol: ServerProtocol::WebSocket,
            addr: DEFAULT_SERVER_ADDR,
            protocol_settings: default(),
            cert: "cert.pem".into(),
            key: "key.pem".into(),
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
                cert,
                key,
            } = entity_mut.take::<Self>().unwrap();

            let config = Self::server_config_encrypted(addr, cert, key)?;

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

    fn server_config_encrypted(
        addr: SocketAddr,
        cert: impl AsRef<Path>,
        key: impl AsRef<Path>,
    ) -> Result<ServerConfig> {
        let cert_chain = CertificateDer::pem_file_iter(cert)?.try_collect::<Vec<_>>()?;
        let key = PrivateKeyDer::from_pem_file(key)?;

        Ok(ServerConfig::builder()
            .with_bind_address(addr)
            .with_identity(websocket::server::Identity::new(cert_chain, key)))
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
