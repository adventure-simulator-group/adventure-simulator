use crate::DEFAULT_SERVER_URL;
use crate::message::{JoinRequest, PlayerInputRequest};
use adventuresim_tactical_core::prelude::*;
use aeronet_replicon::client::{AeronetRepliconClient, AeronetRepliconClientPlugin};
use aeronet_websocket::client::{ClientConfig, WebSocketClient, WebSocketClientPlugin};
use bevy::prelude::*;
use bevy_replicon::prelude::*;

#[derive(Default)]
pub struct AdventureSimulatorClientPlugin;

impl Plugin for AdventureSimulatorClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((WebSocketClientPlugin, AeronetRepliconClientPlugin))
            .add_observer(on_client_added)
            .add_systems(OnEnter(ClientState::Connected), announce_join)
            .add_systems(
                FixedUpdate,
                (send_player_input,).run_if(in_state(ClientState::Connected)),
            );
    }
}

#[derive(Component, Clone, Debug)]
#[require(Name::from("Client"), AeronetRepliconClient)]
pub struct AdventureSimulatorClient {
    pub player_id: u64,
    pub server_url: String,
}

impl Default for AdventureSimulatorClient {
    fn default() -> Self {
        Self {
            player_id: 0,
            server_url: DEFAULT_SERVER_URL.to_string(),
        }
    }
}

fn on_client_added(
    event: On<Add, AdventureSimulatorClient>,
    mut commands: Commands,
    clients: Query<&AdventureSimulatorClient>,
) -> Result {
    let client = clients.get(event.entity)?;

    commands
        .entity(event.entity)
        .queue(WebSocketClient::connect(
            websocket_config(&client.server_url),
            normalize_server_url(&client.server_url),
        ));

    Ok(())
}

fn announce_join(mut commands: Commands, client: Single<&AdventureSimulatorClient>) {
    commands.client_trigger(JoinRequest {
        character_id: CharacterId(client.player_id),
    });
}

fn send_player_input(
    mut commands: Commands,
    players: Query<(&Actions<Player>, &CharacterLook), With<ControlledPlayer>>,
    movements: Query<&Action<input::Movement>>,
    jumps: Query<&Action<input::Jump>>,
) {
    for (actions, look) in &players {
        let movement = movements
            .iter_many(actions)
            .next()
            .map(|movement| **movement);
        let jump = jumps
            .iter_many(actions)
            .next()
            .map(|jump| **jump)
            .unwrap_or(false);

        commands.client_trigger(PlayerInputRequest {
            movement,
            look: Vec2::new(look.yaw, look.pitch),
            jump,
        });
    }
}

#[cfg(target_family = "wasm")]
fn websocket_config(_server_url: &str) -> ClientConfig {
    ClientConfig::default()
}

#[cfg(not(target_family = "wasm"))]
fn websocket_config(server_url: &str) -> ClientConfig {
    if normalize_server_url(server_url).starts_with("wss://") {
        ClientConfig::default()
    } else {
        ClientConfig::builder().with_no_encryption()
    }
}

pub fn normalize_server_url(server_url: &str) -> String {
    if server_url.contains("://") {
        server_url.to_string()
    } else {
        format!("ws://{server_url}")
    }
}
