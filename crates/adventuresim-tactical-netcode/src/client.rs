use crate::DEFAULT_SERVER_URL;
use crate::message::{
    JoinRequest, JumpCommand, PlayerInputRequest, PostureActionRequest, PostureCommand,
    ReconnectCapability, ReconnectToken,
};
use adventuresim_tactical_core::prelude::*;
use aeronet_replicon::client::{AeronetRepliconClient, AeronetRepliconClientPlugin};
use aeronet_websocket::client::{ClientConfig, WebSocketClient, WebSocketClientPlugin};
use bevy::prelude::*;
use bevy_replicon::prelude::*;

const SPRINT_HOLD_THRESHOLD_SECONDS: f32 = 0.25;

#[derive(Default)]
pub struct AdventureSimulatorClientPlugin;

impl Plugin for AdventureSimulatorClientPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WeaponGuardInputState>()
            .init_resource::<ReconnectCredential>()
            .init_resource::<DirectControlState>()
            .init_resource::<DebugForceAttackTrigger>()
            .init_resource::<PlayerInputOverride>()
            .add_plugins((WebSocketClientPlugin, AeronetRepliconClientPlugin))
            .add_observer(on_client_added)
            .add_observer(store_reconnect_capability)
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

/// Kept in the running client application across transport reconnects. It is
/// neither an ECS component nor replicated to peers.
#[derive(Resource, Debug, Clone, Copy, Default)]
struct ReconnectCredential(Option<(CharacterId, ReconnectToken)>);

fn store_reconnect_capability(
    capability: On<ReconnectCapability>,
    mut credential: ResMut<ReconnectCredential>,
) {
    credential.0 = Some((capability.character_id, capability.token));
}

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WeaponGuardInputState {
    pub desired: WeaponGuardState,
}

/// Set via BRP by native diagnostic tooling to force `attack_just_pressed`
/// for one frame without needing real mouse/gamepad input.
/// `update_direct_control_input` recomputes `attack_just_pressed` from raw
/// input unconditionally every frame with no override-checking otherwise, so
/// a plain BRP `insert_resources` on `DirectControlState` itself would get
/// silently clobbered by the very next frame's real (absent) input before
/// `apply_direct_combat_controls` ever reads it - this is a dedicated,
/// consumed-and-cleared trigger instead, the same pattern
/// `DebugDumpWorldTrigger` (`adventuresim-tactical-client/src/debug.rs`)
/// already uses for the same reason.
#[derive(Resource, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Resource)]
pub struct DebugForceAttackTrigger(pub bool);

#[derive(Resource, Debug, Clone, Copy, Reflect)]
#[reflect(Resource)]
pub struct DirectControlState {
    pub pace: MovementPace,
    pub crouch: bool,
    pub jump_charge: bool,
    pub attack_just_pressed: bool,
    pub alternate_attack: bool,
    pub dodge_just_pressed: bool,
    pub quickstep_direction: Vec2,
    pub roll_just_pressed: bool,
    pub downed_align: bool,
    caps_jog: bool,
    sprint: SprintInputState,
    reserved_throw_chord: bool,
    posture_command: PostureCommand,
    posture_control_armed: bool,
    posture_control_consumed: bool,
    gamepad_roll_latched: bool,
    keyboard_roll_pending: bool,
    keyboard_dive_armed: bool,
    keyboard_quickstep_latched: bool,
    space_jump_armed: bool,
    jump_command: JumpCommand,
}

impl Default for DirectControlState {
    fn default() -> Self {
        Self {
            pace: MovementPace::Walk,
            crouch: false,
            jump_charge: false,
            attack_just_pressed: false,
            alternate_attack: false,
            dodge_just_pressed: false,
            quickstep_direction: Vec2::ZERO,
            roll_just_pressed: false,
            downed_align: false,
            caps_jog: false,
            sprint: SprintInputState::default(),
            reserved_throw_chord: false,
            posture_command: PostureCommand::default(),
            posture_control_armed: false,
            posture_control_consumed: false,
            gamepad_roll_latched: false,
            keyboard_roll_pending: false,
            keyboard_dive_armed: false,
            keyboard_quickstep_latched: false,
            space_jump_armed: false,
            jump_command: JumpCommand::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Reflect)]
struct SprintInputState {
    active: bool,
    shift_down: bool,
    held_seconds: f32,
    deactivating_press: bool,
}

impl SprintInputState {
    fn update(&mut self, shift_down: bool, delta_seconds: f32) {
        // The current frame's delta ends at this sample, so it belongs to a
        // key that was already down. Counting it here includes the release
        // frame without pretending a new press began at the frame's start.
        if self.shift_down && !self.deactivating_press {
            self.held_seconds += delta_seconds.max(0.0);
        }
        if shift_down && !self.shift_down {
            self.deactivating_press = self.active;
            self.active = !self.active;
            self.held_seconds = 0.0;
        } else if !shift_down && self.shift_down {
            if !self.deactivating_press && self.held_seconds > SPRINT_HOLD_THRESHOLD_SECONDS {
                self.active = false;
            }
            self.held_seconds = 0.0;
            self.deactivating_press = false;
        }
        self.shift_down = shift_down;
    }

    fn cancel_latch_when_idle(&mut self, moving: bool) {
        if !moving && !self.shift_down {
            self.active = false;
        }
    }
}

/// Optional input supplied by native diagnostic tooling. The request still
/// crosses the ordinary client/server transport and authoritative controller;
/// only the physical keyboard/gamepad sampling is replaced.
#[derive(Resource, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Resource, Default)]
pub struct PlayerInputOverride(pub Option<PlayerInputRequest>);

fn update_direct_control_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    gamepads: Query<&Gamepad>,
    controlled_players: Query<&SkeletonState, With<ControlledPlayer>>,
    mut guard: ResMut<WeaponGuardInputState>,
    mut controls: ResMut<DirectControlState>,
    mut force_attack: ResMut<DebugForceAttackTrigger>,
) {
    if keys.just_pressed(KeyCode::CapsLock) {
        controls.caps_jog = !controls.caps_jog;
    }
    let shift_down = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    controls.sprint.update(shift_down, time.delta_secs());
    let keyboard_direction = Vec2::new(
        (keys.pressed(KeyCode::KeyD) as u8 as f32) - (keys.pressed(KeyCode::KeyA) as u8 as f32),
        (keys.pressed(KeyCode::KeyW) as u8 as f32) - (keys.pressed(KeyCode::KeyS) as u8 as f32),
    );
    let keyboard_moving = keyboard_direction.length_squared() > 0.01;
    let gamepad_direction = gamepads
        .iter()
        .map(|gamepad| {
            Vec2::new(
                gamepad.get(GamepadAxis::LeftStickX).unwrap_or_default(),
                gamepad.get(GamepadAxis::LeftStickY).unwrap_or_default(),
            )
        })
        .max_by(|left, right| left.length_squared().total_cmp(&right.length_squared()))
        .unwrap_or_default();
    let gamepad_moving = gamepad_direction.length_squared() > 0.01;
    let moving = keyboard_moving || gamepad_moving;
    controls.sprint.cancel_latch_when_idle(moving);
    let left_trigger_value = gamepads
        .iter()
        .filter_map(|gamepad| gamepad.get(GamepadButton::LeftTrigger2))
        .fold(0.0_f32, f32::max);
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
    let left_thumb_just_released = gamepads
        .iter()
        .any(|gamepad| gamepad.just_released(GamepadButton::LeftThumb));
    let gamepad_lateral = gamepads
        .iter()
        .filter_map(|gamepad| gamepad.get(GamepadAxis::LeftStickX))
        .max_by(|left, right| left.abs().total_cmp(&right.abs()))
        .unwrap_or_default();
    let downed = controlled_players
        .iter()
        .any(|skeleton| skeleton.body().is_downed());
    let quickstep_grounded = controlled_players
        .iter()
        .next()
        .is_none_or(|skeleton| skeleton.body() == BodyState::Grounded(GroundedPosture::Upright));
    let roll_transitioning = controlled_players.iter().any(|skeleton| {
        matches!(
            skeleton
                .posture_transition()
                .map(|transition| transition.kind()),
            Some(
                PostureTransitionKind::ProneToSupine { .. }
                    | PostureTransitionKind::SupineToProne { .. }
            )
        )
    });

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
    let control_just_released =
        keys.just_released(KeyCode::ControlLeft) || keys.just_released(KeyCode::ControlRight);
    if control_just_pressed || left_thumb_just_pressed {
        controls.posture_control_armed = true;
        controls.posture_control_consumed = false;
    }
    let space_pressed = keys.pressed(KeyCode::Space);
    let space_just_pressed = keys.just_pressed(KeyCode::Space);
    let space_just_released = keys.just_released(KeyCode::Space);
    if control_pressed && space_pressed && (space_just_pressed || control_just_pressed) {
        controls.keyboard_dive_armed = true;
        controls.posture_control_consumed = true;
        controls.space_jump_armed = false;
    }
    let keyboard_dive = space_just_released && controls.keyboard_dive_armed;
    if keyboard_dive {
        controls.keyboard_dive_armed = false;
    }
    let gamepad_dive =
        left_thumb && right_trigger_just_pressed || right_trigger && left_thumb_just_pressed;
    if keyboard_dive || gamepad_dive {
        let movement_direction = if keyboard_moving {
            keyboard_direction
        } else {
            gamepad_direction
        };
        let travel_direction = dive_direction(movement_direction);
        queue_posture_action(
            &mut controls,
            PostureActionRequest::Dive {
                animation_direction: if raised {
                    travel_direction
                } else {
                    DiveDirection::Forward
                },
                travel_direction,
            },
        );
        controls.posture_control_consumed = true;
    }

    if space_just_pressed {
        controls.space_jump_armed = !downed && !controls.keyboard_dive_armed && !keyboard_dive;
    }
    if keyboard_dive || controls.keyboard_dive_armed {
        controls.space_jump_armed = false;
    }
    let keyboard_roll_held = downed
        && !controls.keyboard_dive_armed
        && !keyboard_dive
        && space_pressed
        && (keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::KeyD));
    if roll_transitioning || !keyboard_roll_held {
        controls.keyboard_roll_pending = false;
    }
    let keyboard_roll =
        (keyboard_roll_held && !roll_transitioning && !controls.keyboard_roll_pending)
            .then(|| lateral_roll_action(keyboard_direction.x))
            .flatten();

    let (gamepad_roll_modifier, gamepad_roll_modifier_just_pressed) = controller_roll_modifier(
        left_trigger,
        right_trigger,
        right_trigger_just_pressed,
        right_bumper,
        right_bumper_just_pressed,
    );
    if gamepad_lateral.abs() <= 0.35 || !gamepad_roll_modifier {
        controls.gamepad_roll_latched = false;
    }
    let gamepad_roll = (downed
        && !gamepad_dive
        && gamepad_roll_modifier
        && gamepad_lateral.abs() >= 0.7
        && (!controls.gamepad_roll_latched || gamepad_roll_modifier_just_pressed))
        .then(|| lateral_roll_action(gamepad_lateral))
        .flatten();
    let roll_action = keyboard_roll.or(gamepad_roll);
    let rolling = roll_action.is_some();
    controls.roll_just_pressed = rolling;
    controls.downed_align = downed
        && !controls.keyboard_dive_armed
        && !keyboard_dive
        && !gamepad_dive
        && ((space_pressed && keyboard_direction.x.abs() <= f32::EPSILON)
            || (gamepad_roll_modifier && gamepad_lateral.abs() <= 0.35));
    if let Some(action) = roll_action {
        queue_posture_action(&mut controls, action);
        controls.keyboard_roll_pending = keyboard_roll.is_some();
        controls.gamepad_roll_latched = gamepad_roll.is_some();
        controls.space_jump_armed = false;
    }
    if (control_just_released || left_thumb_just_released) && controls.posture_control_armed {
        if !controls.posture_control_consumed {
            queue_posture_action(&mut controls, PostureActionRequest::Toggle);
        }
        controls.posture_control_armed = false;
        controls.posture_control_consumed = false;
    }

    let mouse_guard = mouse.pressed(MouseButton::Right);
    let mouse_preferred_attack = mouse_guard && mouse.just_pressed(MouseButton::Left);
    let mouse_alternate_attack = mouse_guard && mouse.just_pressed(MouseButton::Middle);
    let controller_attack = left_trigger && right_trigger_just_pressed && !rolling;
    controls.attack_just_pressed =
        mouse_preferred_attack || mouse_alternate_attack || controller_attack || force_attack.0;
    force_attack.0 = false;
    controls.alternate_attack =
        mouse_alternate_attack || (controller_attack && left_trigger_value < 0.95);
    let gamepad_quickstep = left_trigger
        && right_bumper_just_pressed
        && moving
        && !controls.reserved_throw_chord
        && !rolling;
    let keyboard_quickstep_held = raised
        && !downed
        && !controls.keyboard_dive_armed
        && !keyboard_dive
        && !rolling
        && space_pressed
        && keyboard_moving;
    if !keyboard_quickstep_held || !quickstep_grounded {
        controls.keyboard_quickstep_latched = false;
    }
    let keyboard_quickstep =
        keyboard_quickstep_held && quickstep_grounded && !controls.keyboard_quickstep_latched;
    if keyboard_quickstep {
        controls.keyboard_quickstep_latched = true;
        controls.space_jump_armed = false;
    }
    let charging_keyboard_jump = controls.space_jump_armed && space_pressed && !downed && !raised;
    controls.jump_charge = charging_keyboard_jump;
    controls.crouch = raised
        && ((shift_down && !moving)
            || (left_trigger && right_bumper && !moving && !controls.reserved_throw_chord));
    let quickstep_direction = if keyboard_quickstep {
        keyboard_direction
    } else if gamepad_quickstep {
        gamepad_direction
    } else {
        Vec2::ZERO
    }
    .normalize_or_zero();
    let quickstep_requested = quickstep_direction != Vec2::ZERO;
    controls.dodge_just_pressed = quickstep_requested;
    controls.quickstep_direction = quickstep_direction;
    let jump_requested = !quickstep_requested
        && !keyboard_dive
        && !gamepad_dive
        && !rolling
        && (space_just_released && controls.space_jump_armed && !raised
            || (!left_trigger && !left_thumb && right_trigger_just_pressed));
    if jump_requested || quickstep_requested {
        controls.jump_command.sequence = controls.jump_command.sequence.wrapping_add(1);
        controls.jump_command.quickstep = quickstep_requested.then_some(quickstep_direction);
    }
    if space_just_released {
        controls.space_jump_armed = false;
    }
    controls.pace = if keyboard_moving {
        if controls.sprint.active {
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

fn controller_roll_modifier(
    left_trigger: bool,
    right_trigger: bool,
    right_trigger_just_pressed: bool,
    right_bumper: bool,
    right_bumper_just_pressed: bool,
) -> (bool, bool) {
    if left_trigger {
        (right_bumper, right_bumper_just_pressed)
    } else {
        (right_trigger, right_trigger_just_pressed)
    }
}

fn lateral_roll_action(lateral: f32) -> Option<PostureActionRequest> {
    if lateral < 0.0 {
        Some(PostureActionRequest::RollLeft)
    } else if lateral > 0.0 {
        Some(PostureActionRequest::RollRight)
    } else {
        None
    }
}

fn dive_direction(input: Vec2) -> DiveDirection {
    if input.x.abs() > input.y.abs() {
        if input.x < 0.0 {
            DiveDirection::Left
        } else {
            DiveDirection::Right
        }
    } else if input.y < 0.0 {
        DiveDirection::Backward
    } else {
        DiveDirection::Forward
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

fn announce_join(
    mut commands: Commands,
    client: Single<&AdventureSimulatorClient>,
    credential: Res<ReconnectCredential>,
) {
    let character_id = CharacterId(client.player_id);
    commands.client_trigger(JoinRequest {
        character_id,
        reconnect_token: credential
            .0
            .filter(|(stored, _)| *stored == character_id)
            .map(|(_, token)| token),
    });
}

fn send_player_input(
    mut commands: Commands,
    players: Query<(&Actions<Player>, &CharacterLook), With<ControlledPlayer>>,
    movements: Query<&Action<input::Movement>>,
    guard: Res<WeaponGuardInputState>,
    controls: Res<DirectControlState>,
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
            jump: controls.jump_command,
            crouch: controls.crouch,
            jump_charge: controls.jump_charge,
            downed_align: controls.downed_align,
            posture: controls.posture_command,
            pace: controls.pace,
            weapon_guard: guard.desired,
        });
    }
}

fn queue_posture_action(controls: &mut DirectControlState, action: PostureActionRequest) {
    controls.posture_command.sequence = controls.posture_command.sequence.wrapping_add(1);
    controls.posture_command.action = Some(action);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_credential_is_process_owned_not_transport_owned() {
        let credential = ReconnectCredential(Some((CharacterId(42), ReconnectToken([5; 32]))));
        // A transport state transition does not recreate App resources; the
        // same contract is compiled for native and wasm clients.
        assert_eq!(credential.0.unwrap().0, CharacterId(42));
        assert_eq!(credential.0.unwrap().1, ReconnectToken([5; 32]));
    }

    fn input_fixture() -> (World, Schedule) {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.insert_resource(ButtonInput::<KeyCode>::default());
        world.insert_resource(ButtonInput::<MouseButton>::default());
        world.insert_resource(WeaponGuardInputState::default());
        world.insert_resource(DirectControlState::default());
        let mut schedule = Schedule::default();
        schedule.add_systems(update_direct_control_input);
        (world, schedule)
    }

    #[test]
    fn shift_tap_latches_sprint_until_the_next_press() {
        let mut sprint = SprintInputState::default();
        sprint.update(true, 0.0);
        assert!(sprint.active);
        sprint.update(true, 0.1);
        sprint.update(false, 0.0);
        assert!(sprint.active);

        sprint.update(true, 0.0);
        assert!(!sprint.active);
        sprint.update(true, 1.0);
        sprint.update(false, 0.0);
        assert!(!sprint.active);
    }

    #[test]
    fn held_shift_sprints_immediately_and_stops_on_release() {
        let mut sprint = SprintInputState::default();
        sprint.update(true, 0.0);
        assert!(sprint.active);
        sprint.update(true, SPRINT_HOLD_THRESHOLD_SECONDS + 0.01);
        assert!(sprint.active);
        sprint.update(false, 0.0);
        assert!(!sprint.active);
    }

    #[test]
    fn latched_sprint_cancels_when_movement_input_stops() {
        let mut sprint = SprintInputState::default();
        sprint.update(true, 0.0);
        sprint.update(true, 0.1);
        sprint.update(false, 0.0);
        assert!(sprint.active);

        sprint.cancel_latch_when_idle(true);
        assert!(sprint.active);
        sprint.cancel_latch_when_idle(false);
        assert!(!sprint.active);
    }

    #[test]
    fn held_sprint_can_begin_moving_after_an_idle_frame() {
        let mut sprint = SprintInputState::default();
        sprint.update(true, 0.0);
        sprint.cancel_latch_when_idle(false);
        assert!(sprint.active);
    }

    #[test]
    fn shift_release_at_exact_hold_threshold_remains_toggled() {
        let mut sprint = SprintInputState::default();
        sprint.update(true, 0.0);
        sprint.update(false, SPRINT_HOLD_THRESHOLD_SECONDS);
        assert!(sprint.active);
    }

    #[test]
    fn caps_lock_toggles_keyboard_movement_between_walk_and_jog() {
        let (mut world, mut schedule) = input_fixture();
        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::KeyW);
            keys.press(KeyCode::CapsLock);
        }
        schedule.run(&mut world);
        assert_eq!(
            world.resource::<DirectControlState>().pace,
            MovementPace::Jog
        );

        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.clear_just_pressed(KeyCode::CapsLock);
            keys.release(KeyCode::CapsLock);
            keys.clear_just_released(KeyCode::CapsLock);
            keys.press(KeyCode::CapsLock);
        }
        schedule.run(&mut world);
        assert_eq!(
            world.resource::<DirectControlState>().pace,
            MovementPace::Walk
        );
    }

    #[test]
    fn stopping_movement_cancels_sprint_but_preserves_caps_jog() {
        let (mut world, mut schedule) = input_fixture();
        {
            let mut controls = world.resource_mut::<DirectControlState>();
            controls.caps_jog = true;
            controls.sprint.active = true;
        }

        schedule.run(&mut world);
        {
            let controls = world.resource::<DirectControlState>();
            assert!(!controls.sprint.active);
            assert_eq!(controls.pace, MovementPace::Walk);
        }

        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        schedule.run(&mut world);
        let controls = world.resource::<DirectControlState>();
        assert!(!controls.sprint.active);
        assert_eq!(controls.pace, MovementPace::Jog);
    }

    #[test]
    fn posture_toggle_is_emitted_on_release_not_press() {
        let (mut world, mut schedule) = input_fixture();
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ControlLeft);
        schedule.run(&mut world);
        assert_eq!(
            world.resource::<DirectControlState>().posture_command,
            PostureCommand::default()
        );

        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.clear_just_pressed(KeyCode::ControlLeft);
            keys.release(KeyCode::ControlLeft);
        }
        schedule.run(&mut world);
        assert_eq!(
            world.resource::<DirectControlState>().posture_command,
            PostureCommand {
                sequence: 1,
                action: Some(PostureActionRequest::Toggle),
            }
        );
    }

    #[test]
    fn lateral_press_does_not_consume_posture_release() {
        let (mut world, mut schedule) = input_fixture();
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ControlLeft);
        schedule.run(&mut world);

        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.clear_just_pressed(KeyCode::ControlLeft);
            keys.press(KeyCode::KeyA);
        }
        schedule.run(&mut world);
        assert_eq!(
            world.resource::<DirectControlState>().posture_command,
            PostureCommand::default()
        );

        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.clear_just_pressed(KeyCode::KeyA);
            keys.release(KeyCode::ControlLeft);
        }
        schedule.run(&mut world);
        assert_eq!(
            world.resource::<DirectControlState>().posture_command,
            PostureCommand {
                sequence: 1,
                action: Some(PostureActionRequest::Toggle),
            }
        );
    }

    #[test]
    fn space_and_lateral_direction_rolls_when_downed_without_jumping() {
        let (mut world, mut schedule) = input_fixture();
        world.spawn((
            ControlledPlayer::default(),
            SkeletonState::default().with_body_state(BodyState::Prone),
        ));
        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::Space);
            keys.press(KeyCode::KeyA);
        }

        schedule.run(&mut world);
        assert_eq!(
            world.resource::<DirectControlState>().posture_command,
            PostureCommand {
                sequence: 1,
                action: Some(PostureActionRequest::RollLeft),
            }
        );
        assert_eq!(
            world.resource::<DirectControlState>().jump_command,
            JumpCommand::default()
        );
        assert!(world.resource::<DirectControlState>().roll_just_pressed);
        assert!(!world.resource::<DirectControlState>().downed_align);
    }

    #[test]
    fn space_without_lateral_input_requests_downed_camera_alignment() {
        let (mut world, mut schedule) = input_fixture();
        world.spawn((
            ControlledPlayer::default(),
            SkeletonState::default().with_body_state(BodyState::Prone),
        ));
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);

        schedule.run(&mut world);
        let controls = world.resource::<DirectControlState>();
        assert!(controls.downed_align);
        assert!(!controls.roll_just_pressed);
        assert_eq!(controls.posture_command, PostureCommand::default());
    }

    #[test]
    fn held_space_and_lateral_direction_queues_each_roll_after_contact() {
        let (mut world, mut schedule) = input_fixture();
        let player = world
            .spawn((
                ControlledPlayer::default(),
                SkeletonState::default().with_body_state(BodyState::Prone),
            ))
            .id();
        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::Space);
            keys.press(KeyCode::KeyD);
        }
        schedule.run(&mut world);
        assert_eq!(
            world
                .resource::<DirectControlState>()
                .posture_command
                .sequence,
            1
        );

        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.clear_just_pressed(KeyCode::Space);
            keys.clear_just_pressed(KeyCode::KeyD);
        }
        schedule.run(&mut world);
        assert_eq!(
            world
                .resource::<DirectControlState>()
                .posture_command
                .sequence,
            1
        );

        assert!(
            world
                .get_mut::<SkeletonState>(player)
                .unwrap()
                .begin_posture_transition(
                    PostureTransitionKind::ProneToSupine {
                        direction: RollDirection::Right,
                    },
                    0,
                    2,
                )
        );
        schedule.run(&mut world);
        assert_eq!(
            world
                .resource::<DirectControlState>()
                .posture_command
                .sequence,
            1
        );

        world
            .get_mut::<SkeletonState>(player)
            .unwrap()
            .advance_posture_transition(2);
        schedule.run(&mut world);
        assert_eq!(
            world.resource::<DirectControlState>().posture_command,
            PostureCommand {
                sequence: 2,
                action: Some(PostureActionRequest::RollRight),
            }
        );
    }

    #[test]
    fn upright_keyboard_jump_charges_on_press_and_launches_on_release() {
        let (mut world, mut schedule) = input_fixture();
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        schedule.run(&mut world);
        assert!(world.resource::<DirectControlState>().jump_charge);
        assert!(!world.resource::<DirectControlState>().crouch);
        assert_eq!(
            world.resource::<DirectControlState>().jump_command.sequence,
            0
        );

        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.clear_just_pressed(KeyCode::Space);
            keys.release(KeyCode::Space);
        }
        schedule.run(&mut world);
        assert!(!world.resource::<DirectControlState>().jump_charge);
        assert!(!world.resource::<DirectControlState>().crouch);
        assert_eq!(
            world.resource::<DirectControlState>().jump_command.sequence,
            1
        );
    }

    #[test]
    fn aim_and_space_without_movement_do_nothing() {
        let (mut world, mut schedule) = input_fixture();
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Right);
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        schedule.run(&mut world);

        let controls = world.resource::<DirectControlState>();
        assert!(!controls.dodge_just_pressed);
        assert!(!controls.jump_charge);
        assert_eq!(controls.jump_command.sequence, 0);

        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.clear_just_pressed(KeyCode::Space);
            keys.release(KeyCode::Space);
        }
        schedule.run(&mut world);

        let controls = world.resource::<DirectControlState>();
        assert!(!controls.dodge_just_pressed);
        assert!(!controls.jump_charge);
        assert_eq!(controls.jump_command.sequence, 0);
        assert_eq!(controls.jump_command.quickstep, None);
    }

    #[test]
    fn lowering_aim_before_releasing_space_allows_an_ordinary_jump() {
        let (mut world, mut schedule) = input_fixture();
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Right);
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        schedule.run(&mut world);
        assert_eq!(
            world.resource::<DirectControlState>().jump_command.sequence,
            0
        );

        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear_just_pressed(KeyCode::Space);
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .release(MouseButton::Right);
        schedule.run(&mut world);
        assert!(world.resource::<DirectControlState>().jump_charge);

        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::Space);
        schedule.run(&mut world);

        let controls = world.resource::<DirectControlState>();
        assert!(!controls.dodge_just_pressed);
        assert_eq!(controls.jump_command.sequence, 1);
        assert_eq!(controls.jump_command.quickstep, None);
    }

    #[test]
    fn aiming_and_tapping_a_movement_key_never_requests_a_quickstep() {
        let (mut world, mut schedule) = input_fixture();
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Right);
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyA);
        schedule.run(&mut world);
        assert!(!world.resource::<DirectControlState>().dodge_just_pressed);
        assert_eq!(
            world.resource::<DirectControlState>().jump_command.sequence,
            0
        );

        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.clear_just_pressed(KeyCode::KeyA);
            keys.release(KeyCode::KeyA);
        }
        schedule.run(&mut world);

        let controls = world.resource::<DirectControlState>();
        assert!(!controls.dodge_just_pressed);
        assert_eq!(controls.jump_command, JumpCommand::default());
    }

    #[test]
    fn aim_space_and_movement_request_a_quickstep_as_soon_as_the_chord_is_down() {
        let (mut world, mut schedule) = input_fixture();
        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::Space);
            keys.press(KeyCode::KeyD);
        }
        schedule.run(&mut world);
        assert!(!world.resource::<DirectControlState>().dodge_just_pressed);
        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.clear_just_pressed(KeyCode::Space);
            keys.clear_just_pressed(KeyCode::KeyD);
        }
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Right);
        schedule.run(&mut world);

        let controls = world.resource::<DirectControlState>();
        assert!(controls.dodge_just_pressed);
        assert_eq!(controls.jump_command.sequence, 1);
        assert_eq!(controls.jump_command.quickstep, Some(Vec2::X));
        assert!(!controls.jump_charge);
    }

    #[test]
    fn held_quickstep_chord_rearms_in_air_and_repeats_on_ground_contact() {
        let (mut world, mut schedule) = input_fixture();
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Right);
        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::Space);
            keys.press(KeyCode::KeyW);
        }
        schedule.run(&mut world);
        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.clear_just_pressed(KeyCode::Space);
            keys.clear_just_pressed(KeyCode::KeyW);
        }
        schedule.run(&mut world);

        let controls = world.resource::<DirectControlState>();
        assert!(!controls.dodge_just_pressed);
        assert_eq!(controls.jump_command.sequence, 1);
        assert_eq!(controls.jump_command.quickstep, Some(Vec2::Y));

        let player = world
            .spawn((
                ControlledPlayer::default(),
                SkeletonState::default().with_body_state(BodyState::Airborne),
            ))
            .id();
        schedule.run(&mut world);
        assert!(!world.resource::<DirectControlState>().dodge_just_pressed);
        assert_eq!(
            world.resource::<DirectControlState>().jump_command.sequence,
            1
        );

        world
            .get_mut::<SkeletonState>(player)
            .unwrap()
            .transition_body(BodyState::Grounded(GroundedPosture::Upright));
        schedule.run(&mut world);

        let controls = world.resource::<DirectControlState>();
        assert!(controls.dodge_just_pressed);
        assert_eq!(controls.jump_command.sequence, 2);
        assert_eq!(controls.jump_command.quickstep, Some(Vec2::Y));
    }

    #[test]
    fn holding_aim_and_space_then_pressing_movement_requests_a_quickstep() {
        let (mut world, mut schedule) = input_fixture();
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Right);
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        schedule.run(&mut world);
        assert!(!world.resource::<DirectControlState>().dodge_just_pressed);
        assert_eq!(
            world.resource::<DirectControlState>().jump_command.sequence,
            0
        );

        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear_just_pressed(KeyCode::Space);
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyS);
        schedule.run(&mut world);

        let controls = world.resource::<DirectControlState>();
        assert!(controls.dodge_just_pressed);
        assert_eq!(controls.jump_command.sequence, 1);
        assert_eq!(controls.jump_command.quickstep, Some(Vec2::NEG_Y));
    }

    #[test]
    fn jump_charge_preserves_the_selected_movement_pace() {
        let (mut world, mut schedule) = input_fixture();
        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::KeyW);
            keys.press(KeyCode::ShiftLeft);
            keys.press(KeyCode::Space);
        }

        schedule.run(&mut world);
        let controls = world.resource::<DirectControlState>();
        assert!(controls.jump_charge);
        assert!(!controls.crouch);
        assert_eq!(controls.pace, MovementPace::Sprint);
    }

    #[test]
    fn controller_roll_uses_rt_unraised_and_rb_while_lt_is_held() {
        assert_eq!(
            controller_roll_modifier(false, true, true, false, false),
            (true, true)
        );
        assert_eq!(
            controller_roll_modifier(true, true, true, false, false),
            (false, false)
        );
        assert_eq!(
            controller_roll_modifier(true, false, false, true, true),
            (true, true)
        );
    }

    #[test]
    fn dive_direction_uses_the_dominant_axis_and_defaults_forward() {
        assert_eq!(dive_direction(Vec2::ZERO), DiveDirection::Forward);
        assert_eq!(dive_direction(Vec2::new(0.2, 0.8)), DiveDirection::Forward);
        assert_eq!(
            dive_direction(Vec2::new(0.2, -0.8)),
            DiveDirection::Backward
        );
        assert_eq!(dive_direction(Vec2::new(-0.8, 0.2)), DiveDirection::Left);
        assert_eq!(dive_direction(Vec2::new(0.8, 0.2)), DiveDirection::Right);
    }

    #[test]
    fn dive_waits_for_space_release_and_defaults_forward_without_aim() {
        let (mut world, mut schedule) = input_fixture();
        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::ControlLeft);
            keys.press(KeyCode::Space);
            keys.press(KeyCode::KeyD);
        }

        schedule.run(&mut world);
        assert_eq!(
            world.resource::<DirectControlState>().posture_command,
            PostureCommand::default()
        );
        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.clear_just_pressed(KeyCode::ControlLeft);
            keys.clear_just_pressed(KeyCode::Space);
            keys.clear_just_pressed(KeyCode::KeyD);
            keys.release(KeyCode::Space);
        }
        schedule.run(&mut world);
        assert_eq!(
            world.resource::<DirectControlState>().posture_command,
            PostureCommand {
                sequence: 1,
                action: Some(PostureActionRequest::Dive {
                    animation_direction: DiveDirection::Forward,
                    travel_direction: DiveDirection::Right,
                }),
            }
        );
        assert_eq!(
            world.resource::<DirectControlState>().jump_command,
            JumpCommand::default()
        );
    }

    #[test]
    fn aimed_dive_uses_held_movement_direction_on_space_release() {
        let (mut world, mut schedule) = input_fixture();
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Right);
        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::ControlLeft);
            keys.press(KeyCode::Space);
            keys.press(KeyCode::KeyD);
        }
        schedule.run(&mut world);
        {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.clear_just_pressed(KeyCode::ControlLeft);
            keys.clear_just_pressed(KeyCode::Space);
            keys.clear_just_pressed(KeyCode::KeyD);
            keys.release(KeyCode::Space);
        }
        schedule.run(&mut world);
        assert_eq!(
            world
                .resource::<DirectControlState>()
                .posture_command
                .action,
            Some(PostureActionRequest::Dive {
                animation_direction: DiveDirection::Right,
                travel_direction: DiveDirection::Right,
            })
        );
    }
}
