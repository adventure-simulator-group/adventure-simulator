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
        app.init_resource::<WeaponGuardInputState>()
            .init_resource::<DirectControlState>()
            .init_resource::<PlayerInputOverride>()
            .add_plugins((WebSocketClientPlugin, AeronetRepliconClientPlugin))
            .add_observer(on_client_added)
            .add_systems(OnEnter(ClientState::Connected), announce_join)
            .add_systems(
                PreUpdate,
                update_direct_control_input.after(bevy::input::InputSystems),
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

#[derive(Resource, Debug, Clone, Copy)]
pub struct DirectControlState {
    pub pace: MovementPace,
    pub jump: bool,
    pub crouch: bool,
    pub prone: bool,
    pub attack_just_pressed: bool,
    pub dodge_just_pressed: bool,
    caps_jog: bool,
    guarded_space: bool,
    reserved_throw_chord: bool,
}

impl Default for DirectControlState {
    fn default() -> Self {
        Self {
            pace: MovementPace::Walk,
            jump: false,
            crouch: false,
            prone: false,
            attack_just_pressed: false,
            dodge_just_pressed: false,
            caps_jog: false,
            guarded_space: false,
            reserved_throw_chord: false,
        }
    }
}

/// Optional input supplied by native diagnostic tooling. The request still
/// crosses the ordinary client/server transport and authoritative controller;
/// only the physical keyboard/gamepad sampling is replaced.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct PlayerInputOverride(pub Option<PlayerInputRequest>);

fn update_direct_control_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    gamepads: Query<&Gamepad>,
    mut guard: ResMut<WeaponGuardInputState>,
    mut controls: ResMut<DirectControlState>,
) {
    if keys.just_pressed(KeyCode::CapsLock) {
        controls.caps_jog = !controls.caps_jog;
    }
    let keyboard_moving = [KeyCode::KeyW, KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyD]
        .into_iter()
        .any(|key| keys.pressed(key));
    let gamepad_moving = gamepads.iter().any(|gamepad| {
        Vec2::new(
            gamepad.get(GamepadAxis::LeftStickX).unwrap_or_default(),
            gamepad.get(GamepadAxis::LeftStickY).unwrap_or_default(),
        )
        .length_squared()
            > 0.01
    });
    let moving = keyboard_moving || gamepad_moving;
    let left_trigger = gamepads
        .iter()
        .any(|gamepad| gamepad.pressed(GamepadButton::LeftTrigger2));
    let left_trigger_just_pressed = gamepads
        .iter()
        .any(|gamepad| gamepad.just_pressed(GamepadButton::LeftTrigger2));
    let right_bumper = gamepads
        .iter()
        .any(|gamepad| gamepad.pressed(GamepadButton::RightTrigger));
    let right_bumper_just_pressed = gamepads
        .iter()
        .any(|gamepad| gamepad.just_pressed(GamepadButton::RightTrigger));
    let right_trigger_just_pressed = gamepads
        .iter()
        .any(|gamepad| gamepad.just_pressed(GamepadButton::RightTrigger2));
    let right_trigger = gamepads
        .iter()
        .any(|gamepad| gamepad.pressed(GamepadButton::RightTrigger2));
    let left_thumb = gamepads
        .iter()
        .any(|gamepad| gamepad.pressed(GamepadButton::LeftThumb));
    let left_thumb_just_pressed = gamepads
        .iter()
        .any(|gamepad| gamepad.just_pressed(GamepadButton::LeftThumb));

    let raised = mouse.pressed(MouseButton::Right) || left_trigger;
    if right_bumper && left_trigger_just_pressed {
        controls.reserved_throw_chord = true;
    }
    if !right_bumper || !left_trigger {
        controls.reserved_throw_chord = false;
    }
    guard.desired = if raised {
        WeaponGuardState::Raised
    } else {
        WeaponGuardState::Lowered
    };

    let control_pressed = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let control_just_pressed =
        keys.just_pressed(KeyCode::ControlLeft) || keys.just_pressed(KeyCode::ControlRight);
    let keyboard_dive = control_pressed && keys.just_pressed(KeyCode::Space)
        || keys.pressed(KeyCode::Space) && control_just_pressed;
    let gamepad_dive =
        left_thumb && right_trigger_just_pressed || right_trigger && left_thumb_just_pressed;
    if !keyboard_dive && control_just_pressed {
        controls.prone = !controls.prone;
    }
    if !gamepad_dive && left_thumb_just_pressed {
        controls.prone = !controls.prone;
    }
    if keyboard_dive || gamepad_dive {
        controls.prone = false;
    }

    if raised && keys.just_pressed(KeyCode::Space) {
        controls.guarded_space = true;
    }
    let keyboard_dodge = raised && controls.guarded_space && keys.just_released(KeyCode::Space);
    if keys.just_released(KeyCode::Space) {
        controls.guarded_space = false;
    }
    controls.attack_just_pressed = raised
        && (mouse.just_pressed(MouseButton::Left) || (left_trigger && right_trigger_just_pressed));
    controls.dodge_just_pressed = keyboard_dodge
        || (left_trigger && right_bumper_just_pressed && moving && !controls.reserved_throw_chord);
    controls.crouch = raised
        && (keys.pressed(KeyCode::Space)
            || (keys.pressed(KeyCode::ShiftLeft) && !moving)
            || (left_trigger && right_bumper && !moving && !controls.reserved_throw_chord));
    controls.jump = !raised
        && !keyboard_dive
        && !gamepad_dive
        && (keys.just_pressed(KeyCode::Space)
            || (!left_trigger && !left_thumb && right_trigger_just_pressed));
    controls.pace = if keyboard_moving {
        if keys.pressed(KeyCode::ShiftLeft) {
            MovementPace::Sprint
        } else if controls.caps_jog {
            MovementPace::Jog
        } else {
            MovementPace::Walk
        }
    } else if gamepad_moving {
        MovementPace::Sprint
    } else {
        MovementPace::Walk
    };
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
    guard: Res<WeaponGuardInputState>,
    mut controls: ResMut<DirectControlState>,
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
        commands.client_trigger(PlayerInputRequest {
            movement,
            look: Vec2::new(look.yaw, look.pitch),
            jump: controls.jump,
            crouch: controls.crouch,
            prone: controls.prone,
            pace: controls.pace,
            weapon_guard: guard.desired,
        });
        controls.jump = false;
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
