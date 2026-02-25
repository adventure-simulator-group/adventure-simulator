use std::net::SocketAddr;
use std::time::Duration;

use crate::prelude::ProtocolSettings;
use crate::protocol::SEND_INTERVAL;
use crate::{DEFAULT_CLIENT_ADDR, DEFAULT_SERVER_ADDR, FIXED_TICK_DURATION};
use adventure_simulator_core::player::Player;
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
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

        // On client, players should have interpolated components for better visuals.
        app.register_required_components::<Player, Interpolated>();
    }
}

#[derive(Component, Clone, Debug)]
#[component(immutable, on_add = Self::on_add)]
#[require(Name = Name::from("Client"), Client)]
pub struct AdventureSimulatorClient {
    pub id: u64,
    pub server_addr: SocketAddr,
    pub addr: SocketAddr,
    pub protocol_settings: ProtocolSettings,
}

impl Default for AdventureSimulatorClient {
    fn default() -> Self {
        Self {
            id: 0,
            server_addr: DEFAULT_SERVER_ADDR,
            addr: DEFAULT_CLIENT_ADDR,
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
                protocol_settings,
            } = entity_mut.take::<Self>().unwrap();

            // use dummy zeroed key explicitly here.
            let auth = Authentication::Manual {
                server_addr: server_addr,
                client_id: client_id,
                private_key: protocol_settings.private_key.0,
                protocol_id: protocol_settings.id,
            };
            let netcode_config = NetcodeConfig {
                // Make sure that the server times out clients when their connection is closed
                client_timeout_secs: 3,
                token_expire_secs: -1,
                ..default()
            };
            entity_mut.insert((
                InputTimelineConfig::default()
                    .with_sync_config(SyncConfig {
                        jitter_margin: SEND_INTERVAL,
                        ..default()
                    })
                    .with_input_delay(InputDelayConfig::no_prediction()),
                NetcodeClient::new(auth, netcode_config)?,
                LocalAddr(addr),
                UdpIo::default(),
                PeerAddr(server_addr),
                ReplicationReceiver::default(),
                ReplicationSender::new(SEND_INTERVAL, SendUpdatesMode::SinceLastAck, false),
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
