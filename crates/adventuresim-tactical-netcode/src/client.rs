use crate::DEFAULT_SERVER_URL;
use crate::message::{JoinRequest, PlayerInputRequest};
use adventuresim_tactical_core::prelude::*;
use aeronet_replicon::client::{AeronetRepliconClient, AeronetRepliconClientPlugin};
use aeronet_websocket::client::{ClientConfig, WebSocketClient, WebSocketClientPlugin};
use bevy::{input::mouse::AccumulatedMouseScroll, prelude::*};
use bevy_replicon::prelude::*;

#[derive(Default)]
pub struct AdventureSimulatorClientPlugin;

impl Plugin for AdventureSimulatorClientPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WeaponGuardInputState>()
            .init_resource::<PlayerInputOverride>()
            .add_plugins((WebSocketClientPlugin, AeronetRepliconClientPlugin))
            .add_observer(on_client_added)
            .add_systems(OnEnter(ClientState::Connected), announce_join)
            .add_systems(
                PreUpdate,
                update_weapon_guard_input.after(bevy::input::InputSystems),
            )
            .add_systems(
                FixedUpdate,
                (send_player_input,).run_if(in_state(ClientState::Connected)),
            );
    }
}

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WeaponGuardInputState {
    pub desired: WeaponGuardState,
}

/// Optional input supplied by native diagnostic tooling. The request still
/// crosses the ordinary client/server transport and authoritative controller;
/// only the physical keyboard/gamepad sampling is replaced.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct PlayerInputOverride(pub Option<PlayerInputRequest>);

impl WeaponGuardInputState {
    pub fn apply_controls(&mut self, wheel_y: f32, right_thumb_pressed: bool) {
        if wheel_y > 0.0 {
            self.desired = WeaponGuardState::Raised;
        } else if wheel_y < 0.0 {
            self.desired = WeaponGuardState::Lowered;
        }
        if right_thumb_pressed {
            self.desired = match self.desired {
                WeaponGuardState::Lowered => WeaponGuardState::Raised,
                WeaponGuardState::Raised => WeaponGuardState::Lowered,
            };
        }
    }
}

fn update_weapon_guard_input(
    scroll: Res<AccumulatedMouseScroll>,
    gamepads: Query<&Gamepad>,
    mut guard: ResMut<WeaponGuardInputState>,
) {
    guard.apply_controls(
        scroll.delta.y,
        gamepads
            .iter()
            .any(|gamepad| gamepad.just_pressed(GamepadButton::RightThumb)),
    );
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
    guard: Res<WeaponGuardInputState>,
    scripted: Res<PlayerInputOverride>,
) {
    for (actions, look) in &players {
        if let Some(request) = scripted.0 {
            commands.client_trigger(request);
            continue;
        }
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
            weapon_guard: guard.desired,
        });
    }
}

#[cfg(test)]
mod weapon_guard_input_tests {
    use super::*;

    #[test]
    fn guard_defaults_lowered_and_wheel_is_idempotent() {
        let mut state = WeaponGuardInputState::default();
        assert_eq!(state.desired, WeaponGuardState::Lowered);
        state.apply_controls(1.0, false);
        state.apply_controls(1.0, false);
        assert_eq!(state.desired, WeaponGuardState::Raised);
        state.apply_controls(-1.0, false);
        state.apply_controls(-1.0, false);
        assert_eq!(state.desired, WeaponGuardState::Lowered);
    }

    #[test]
    fn right_thumb_toggles_only_on_reported_press_edge() {
        let mut state = WeaponGuardInputState::default();
        state.apply_controls(0.0, true);
        assert_eq!(state.desired, WeaponGuardState::Raised);
        state.apply_controls(0.0, false);
        state.apply_controls(0.0, false);
        assert_eq!(state.desired, WeaponGuardState::Raised);
        state.apply_controls(0.0, true);
        assert_eq!(state.desired, WeaponGuardState::Lowered);
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
