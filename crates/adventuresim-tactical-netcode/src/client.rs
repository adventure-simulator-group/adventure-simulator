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
            .init_resource::<MovementGaitInputState>()
            .init_resource::<PlayerInputOverride>()
            .add_plugins((WebSocketClientPlugin, AeronetRepliconClientPlugin))
            .add_observer(on_client_added)
            .add_systems(OnEnter(ClientState::Connected), announce_join)
            .add_systems(
                PreUpdate,
                (update_weapon_guard_input, update_movement_gait_input)
                    .after(bevy::input::InputSystems),
            )
            .add_systems(
                FixedUpdate,
                (send_player_input,).run_if(in_state(ClientState::Connected)),
            );
    }
}

const SPRINT_HOLD_THRESHOLD_SECONDS: f32 = 0.25;

/// Local keyboard interpretation for the walk/jog toggle and hybrid
/// hold-or-toggle sprint control. The resolved gait is still validated and
/// applied authoritatively by the server.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct MovementGaitInputState {
    base_gait: MovementGait,
    sprint_active: bool,
    shift_held_seconds: Option<f32>,
}

impl Default for MovementGaitInputState {
    fn default() -> Self {
        Self {
            base_gait: MovementGait::Jog,
            sprint_active: false,
            shift_held_seconds: None,
        }
    }
}

impl MovementGaitInputState {
    #[must_use]
    pub fn desired_gait(&self) -> MovementGait {
        if self.sprint_active {
            MovementGait::Sprint
        } else {
            self.base_gait
        }
    }

    fn apply_input(
        &mut self,
        caps_lock_pressed: bool,
        shift_pressed: bool,
        shift_released: bool,
        shift_down: bool,
        delta_seconds: f32,
    ) {
        if caps_lock_pressed {
            self.base_gait = match self.base_gait {
                MovementGait::Walk => MovementGait::Jog,
                MovementGait::Jog | MovementGait::Sprint => MovementGait::Walk,
            };
        }

        if shift_pressed {
            if self.sprint_active {
                // A latched sprint toggles off as soon as Shift is pressed
                // again, regardless of how long the second press is held.
                self.sprint_active = false;
                self.shift_held_seconds = None;
            } else {
                self.sprint_active = true;
                self.shift_held_seconds = Some(0.0);
            }
        }
        if shift_down && let Some(held) = &mut self.shift_held_seconds {
            *held += delta_seconds.max(0.0);
        }
        if shift_released
            && self
                .shift_held_seconds
                .take()
                .is_some_and(|held| held > SPRINT_HOLD_THRESHOLD_SECONDS)
        {
            self.sprint_active = false;
        }
    }
}

fn update_movement_gait_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut gait: ResMut<MovementGaitInputState>,
) {
    let shift_pressed =
        keyboard.just_pressed(KeyCode::ShiftLeft) || keyboard.just_pressed(KeyCode::ShiftRight);
    let shift_released = (keyboard.just_released(KeyCode::ShiftLeft)
        || keyboard.just_released(KeyCode::ShiftRight))
        && !keyboard.pressed(KeyCode::ShiftLeft)
        && !keyboard.pressed(KeyCode::ShiftRight);
    let shift_down = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
    gait.apply_input(
        keyboard.just_pressed(KeyCode::CapsLock),
        shift_pressed,
        shift_released,
        shift_down,
        time.delta_secs(),
    );
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
    gait: Res<MovementGaitInputState>,
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
            gait: gait.desired_gait(),
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

#[cfg(test)]
mod movement_gait_input_tests {
    use super::*;

    #[test]
    fn caps_lock_toggles_walk_and_jog() {
        let mut state = MovementGaitInputState::default();
        assert_eq!(state.desired_gait(), MovementGait::Jog);
        state.apply_input(true, false, false, false, 0.0);
        assert_eq!(state.desired_gait(), MovementGait::Walk);
        state.apply_input(true, false, false, false, 0.0);
        assert_eq!(state.desired_gait(), MovementGait::Jog);
    }

    #[test]
    fn quick_shift_tap_latches_sprint_until_next_press() {
        let mut state = MovementGaitInputState::default();
        state.apply_input(false, true, false, true, 0.0);
        assert_eq!(state.desired_gait(), MovementGait::Sprint);
        state.apply_input(false, false, false, true, 0.2);
        state.apply_input(false, false, true, false, 0.0);
        assert_eq!(state.desired_gait(), MovementGait::Sprint);

        state.apply_input(false, true, false, true, 0.0);
        assert_eq!(state.desired_gait(), MovementGait::Jog);
        state.apply_input(false, false, true, false, 1.0);
        assert_eq!(state.desired_gait(), MovementGait::Jog);
    }

    #[test]
    fn held_shift_sprints_immediately_and_stops_on_release() {
        let mut state = MovementGaitInputState::default();
        state.apply_input(false, true, false, true, 0.0);
        assert_eq!(state.desired_gait(), MovementGait::Sprint);
        state.apply_input(false, false, false, true, 0.251);
        assert_eq!(state.desired_gait(), MovementGait::Sprint);
        state.apply_input(false, false, true, false, 0.0);
        assert_eq!(state.desired_gait(), MovementGait::Jog);
    }

    #[test]
    fn exactly_quarter_second_remains_a_toggle() {
        let mut state = MovementGaitInputState::default();
        state.apply_input(false, true, false, true, 0.0);
        state.apply_input(false, false, false, true, 0.25);
        state.apply_input(false, false, true, false, 0.0);
        assert_eq!(state.desired_gait(), MovementGait::Sprint);
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
