use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::prelude::*;
use serde::Deserialize;

use crate::token::HexConnectToken;
use crate::{DEFAULT_CLIENT_ADDR, FIXED_TICK_DURATION};
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use lightyear::netcode::{ConnectToken, NetcodeClient};
use lightyear::prelude::client::*;
use lightyear::prelude::*;

#[derive(Default)]
pub struct AdventureSimulatorClientPlugin;

impl Plugin for AdventureSimulatorClientPlugin {
    fn build(&self, app: &mut App) {
        let tick_duration = Duration::from_secs_f64(FIXED_TICK_DURATION);
        app.add_plugins(ClientPlugins { tick_duration });
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub enum ClientProtocol {
    Udp,
    WebTransport { certificate_digest: String },
    WebSocket,
}

#[derive(Component, Clone, Debug)]
#[component(immutable, on_add = Self::on_add)]
#[require(Name = Name::from("Client"), Client)]
pub struct AdventureSimulatorClient {
    pub token: HexConnectToken,
    pub addr: SocketAddr,
    pub ca_cert: Option<PathBuf>,
    pub cert: Option<PathBuf>,
    pub key: Option<PathBuf>,
}

impl Default for AdventureSimulatorClient {
    fn default() -> Self {
        Self {
            token: HexConnectToken::default(),
            addr: DEFAULT_CLIENT_ADDR,
            ca_cert: None,
            cert: None,
            key: None,
        }
    }
}

impl AdventureSimulatorClient {
    fn on_add(mut world: DeferredWorld, context: HookContext) {
        let entity = context.entity;
        world.commands().queue(move |world: &mut World| -> Result {
            let mut entity_mut = world.entity_mut(entity);
            let AdventureSimulatorClient {
                token,
                addr,
                ca_cert,
                cert,
                key,
            } = entity_mut.take::<Self>().unwrap();

            let token = ConnectToken::try_from_bytes(&*token)?;
            let auth = Authentication::Token(token);

            let netcode_config = match (ca_cert, cert, key) {
                #[cfg(not(target_family = "wasm"))]
                (Some(ca_cert), Some(cert), Some(key)) => {
                    Self::client_config_encrypted_local(ca_cert, cert, key)?
                },
                #[cfg(target_family = "wasm")]
                (Some(_), Some(_), Some(_)) => {
                    warn!("Can't create a safe connection using a local ca_cert, cert and key on wasm. Only native SSL is available on wasm.");
                    Self::client_config_encrypted()
                },
                _ => Self::client_config_encrypted()
            };

            entity_mut.insert((
                LocalAddr(addr),
                ReplicationReceiver::default(),
                NetcodeClient::new(auth, netcode_config)?,
                WebSocketClientIo { config },
            ));

            world.trigger(Connect { entity });
            Ok(())
        });
    }

    fn client_config_encrypted() -> ClientConfig {
        #[cfg(target_family = "wasm")]
        {
            ClientConfig::default()
        }
        #[cfg(not(target_family = "wasm"))]
        {
            ClientConfig::builder().with_native_certs()
        }
    }

    #[cfg(not(target_family = "wasm"))]
    fn client_config_encrypted_local(
        ca_cert: impl AsRef<Path>,
        cert: impl AsRef<Path>,
        key: impl AsRef<Path>,
    ) -> Result<ClientConfig> {
        use lightyear::prelude::rustls::pki_types::{
            pem::PemObject, CertificateDer, PrivateKeyDer,
        };

        let ca_cert_chain = CertificateDer::pem_file_iter(ca_cert)?.try_collect::<Vec<_>>()?;
        let mut root_store = rustls::RootCertStore::empty();
        root_store.add_parsable_certificates(ca_cert_chain);

        let cert_chain = CertificateDer::pem_file_iter(cert)?.try_collect::<Vec<_>>()?;
        let key = PrivateKeyDer::from_pem_file(key)?;

        Ok(ClientConfig::builder().with_tls_config(
            rustls::ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_client_auth_cert(cert_chain, key)?,
        ))
    }
}

// /// Read certificate digest from alternate sources, for WASM builds.
// // #[cfg(target_family = "wasm")]
// #[allow(unreachable_patterns)]
// pub fn modify_digest_on_wasm(client_settings: &mut ClientSettings) -> Option<String> {
//     if let Some(new_digest) = get_digest_on_wasm() {
//         match &client_settings.transport {
//             ClientTransports::WebTransport { certificate_digest } => {
//                 client_settings.transport = ClientTransports::WebTransport {
//                     certificate_digest: new_digest.clone(),
//                 };
//                 Some(new_digest)
//             }
//             // This could be unreachable if only WebTransport feature is enabled.
//             // hence we suppress this warning with the allow directive above.
//             _ => None,
//         }
//     } else {
//         None
//     }
// }

// // #[cfg(target_family = "wasm")]
// pub fn get_digest_on_wasm() -> Option<String> {
//     let window = web_sys::window().expect("expected window");

//     if let Ok(obj) = window.location().hash() {
//         info!("Using cert digest from window.location().hash()");
//         let cd = obj.replace("#", "");
//         if cd.len() > 10 {
//             // lazy sanity check.
//             return Some(cd);
//         }
//     }

//     if let Some(obj) = window.get("CERT_DIGEST") {
//         info!("Using cert digest from window.CERT_DIGEST");
//         return Some(obj.as_string().expect("CERT_DIGEST should be a string"));
//     }

//     None
// }
