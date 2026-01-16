use std::net::SocketAddr;
use std::time::Duration;

use bevy::prelude::*;
use serde::Deserialize;

use crate::prelude::ProtocolSettings;
use crate::token::HexConnectToken;
use crate::{DEFAULT_CLIENT_ADDR, FIXED_TICK_DURATION};
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use lightyear::netcode::client_plugin::NetcodeConfig;
use lightyear::netcode::NetcodeClient;
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
    pub protocol: ClientProtocol,
    pub protocol_settings: ProtocolSettings,
}

impl Default for AdventureSimulatorClient {
    fn default() -> Self {
        Self {
            token: HexConnectToken::default(),
            addr: DEFAULT_CLIENT_ADDR,
            protocol: ClientProtocol::Udp,
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
                token,
                addr,
                protocol,
                protocol_settings,
            } = entity_mut.take::<Self>().unwrap();

            let token = ConnectToken::try_from_bytes(&*token)?;
            let auth = Authentication::Token(token);
            let netcode_config = default();

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
