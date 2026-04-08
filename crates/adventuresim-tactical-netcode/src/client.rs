use crate::message::{AttackCommand, JoinRequest, PlayerInputMessage};
use crate::DEFAULT_SERVER_URL;
use adventuresim_tactical_core::prelude::*;
use aeronet::io::connection::Disconnected;
use aeronet_replicon::client::{AeronetRepliconClient, AeronetRepliconClientPlugin};
use aeronet_websocket::client::{ClientConfig, WebSocketClient, WebSocketClientPlugin};
use bevy::prelude::*;
use bevy_replicon::prelude::*;

#[derive(Default)]
pub struct AdventureSimulatorClientPlugin;

impl Plugin for AdventureSimulatorClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((WebSocketClientPlugin, AeronetRepliconClientPlugin))
            .init_resource::<SampledPlayerInput>()
            .init_resource::<JoinAnnouncementState>()
            .init_resource::<ConnectionRetry>()
            .add_systems(
                Update,
                (
                    connect_added_clients.run_if(client_not_connecting),
                    reset_connection_retry.run_if(client_connected),
                    announce_join_once.run_if(client_connected),
                    sample_player_input.run_if(client_connected),
                    send_attack.run_if(client_connected),
                    tick_connection_retry,
                ),
            )
            .add_systems(
                FixedUpdate,
                send_player_input.run_if(client_connected),
            )
            .add_observer(handle_disconnect);
    }
}

#[derive(Resource, Debug, Default, Clone, Copy)]
struct SampledPlayerInput {
    message: PlayerInputMessage,
}

#[derive(Resource, Debug, Default, Clone, Copy)]
struct JoinAnnouncementState {
    sent: bool,
}

#[derive(Resource, Debug)]
struct ConnectionRetry {
    attempts: u32,
    max_attempts: u32,
    retry_delay: Timer,
    waiting_to_retry: bool,
}

impl Default for ConnectionRetry {
    fn default() -> Self {
        Self {
            attempts: 0,
            max_attempts: 5,
            retry_delay: Timer::from_seconds(2.0, TimerMode::Once),
            waiting_to_retry: false,
        }
    }
}

#[derive(Component, Clone, Debug)]
#[require(Name = Name::from("Client"))]
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

fn connect_added_clients(
    mut commands: Commands,
    clients: Query<(Entity, &AdventureSimulatorClient), Added<AdventureSimulatorClient>>,
) {
    for (entity, client) in &clients {
        commands.entity(entity).insert(AeronetRepliconClient);
        commands
            .entity(entity)
            .queue(WebSocketClient::connect(websocket_config(&client.server_url), normalize_server_url(&client.server_url)));
    }
}

fn announce_join_once(
    mut commands: Commands,
    client: Single<&AdventureSimulatorClient>,
    mut join_state: ResMut<JoinAnnouncementState>,
) {
    if join_state.sent {
        return;
    }

    commands.client_trigger(JoinRequest {
        player_id: client.player_id,
    });
    join_state.sent = true;
}

fn reset_connection_retry(
    mut retry: ResMut<ConnectionRetry>,
) {
    if retry.attempts == 0 && !retry.waiting_to_retry {
        return;
    }
    retry.attempts = 0;
    retry.waiting_to_retry = false;
    info!("Connected to tactical server");
}

fn handle_disconnect(
    trigger: On<Disconnected>,
    clients: Query<(), With<AdventureSimulatorClient>>,
    mut retry: ResMut<ConnectionRetry>,
    mut join_state: ResMut<JoinAnnouncementState>,
) {
    if clients.get(trigger.event_target()).is_err() {
        return;
    }

    join_state.sent = false;

    if retry.attempts >= retry.max_attempts {
        error!(
            "Disconnected from tactical server after {} attempts: {:?}",
            retry.max_attempts, trigger.reason
        );
        retry.waiting_to_retry = false;
        return;
    }

    warn!(
        "Disconnected from tactical server: {:?}. Retrying in 2s... (attempt {}/{})",
        trigger.reason,
        retry.attempts + 1,
        retry.max_attempts,
    );
    retry.waiting_to_retry = true;
    retry.retry_delay.reset();
}

fn tick_connection_retry(
    time: Res<Time>,
    mut retry: ResMut<ConnectionRetry>,
    mut commands: Commands,
    client: Single<(Entity, &AdventureSimulatorClient)>,
    client_state: Option<Res<State<ClientState>>>,
) {
    if !retry.waiting_to_retry {
        return;
    }

    if matches!(
        client_state.as_deref().map(State::get),
        Some(ClientState::Connecting | ClientState::Connected)
    ) {
        return;
    }

    retry.retry_delay.tick(time.delta());
    if !retry.retry_delay.just_finished() {
        return;
    }

    retry.attempts += 1;
    retry.waiting_to_retry = false;

    let (entity, client) = *client;
    info!(
        "Attempting tactical server reconnect {}/{} to {}",
        retry.attempts,
        retry.max_attempts,
        normalize_server_url(&client.server_url),
    );
    commands
        .entity(entity)
        .insert(AeronetRepliconClient)
        .queue(WebSocketClient::connect(
            websocket_config(&client.server_url),
            normalize_server_url(&client.server_url),
        ));
}

fn sample_player_input(
    mut sampled_input: ResMut<SampledPlayerInput>,
    players: Query<(&Actions<Player>, &CharacterLook), With<ControlledPlayer>>,
    movements: Query<&Action<input::Movement>>,
    jumps: Query<&Action<input::Jump>>,
) {
    let Some((actions, look)) = players.iter().next() else {
        sampled_input.message = PlayerInputMessage::default();
        return;
    };

    let movement = movements
        .iter_many(actions)
        .next()
        .map(|movement| **movement)
        .unwrap_or_default();
    let jump = jumps
        .iter_many(actions)
        .next()
        .map(|jump| **jump)
        .unwrap_or(false);

    sampled_input.message = PlayerInputMessage {
        movement,
        look: Vec2::new(look.yaw, look.pitch),
        jump,
    };
}

fn send_player_input(
    mut commands: Commands,
    sampled_input: Res<SampledPlayerInput>,
    players: Query<(), With<ControlledPlayer>>,
) {
    if players.is_empty() {
        return;
    }

    commands.client_trigger(sampled_input.message);
}

fn send_attack(
    mut commands: Commands,
    players: Query<&Actions<Player>, With<ControlledPlayer>>,
    attacks: Query<&ActionEvents, With<Action<Attack>>>,
) {
    for actions in &players {
        let Some(events) = attacks.iter_many(actions).next() else {
            continue;
        };
        if events.contains(ActionEvents::START) {
            commands.client_trigger(AttackCommand);
        }
    }
}

fn client_connected(client_state: Option<Res<State<ClientState>>>) -> bool {
    matches!(
        client_state.as_deref().map(State::get),
        Some(ClientState::Connected)
    )
}

fn client_not_connecting(client_state: Option<Res<State<ClientState>>>) -> bool {
    !matches!(
        client_state.as_deref().map(State::get),
        Some(ClientState::Connecting)
    )
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
