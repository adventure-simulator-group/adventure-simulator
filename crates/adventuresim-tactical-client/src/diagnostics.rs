use std::{fs::File, path::Path};

use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::prelude::*;
use bevy::{
    app::AppExit,
    input::{
        InputSystems,
        mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    },
    prelude::*,
    render::{Render, RenderApp, RenderSystems},
};
use serde::Deserialize;

use crate::{
    animation::{AnimationDiagnosticLog, DiagnosticInputStatus, RenderScheduleTelemetry},
    player::ClientPlayer,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct InputScript {
    commands: Vec<ScriptCommand>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum ScriptCommand {
    Rotate {
        degrees_right: f32,
    },
    Move {
        #[serde(default)]
        direction: MoveDirection,
        input_speed: f32,
        duration_seconds: f32,
    },
    Wait {
        duration_seconds: f32,
    },
    WaitForSignal {
        path: String,
    },
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MoveDirection {
    #[default]
    Forward,
    Backward,
    Left,
    Right,
}

impl MoveDirection {
    fn vector(self) -> Vec2 {
        match self {
            Self::Forward => Vec2::Y,
            Self::Backward => Vec2::NEG_Y,
            Self::Left => Vec2::NEG_X,
            Self::Right => Vec2::X,
        }
    }
}

#[derive(Resource, Debug)]
struct ScriptedInput {
    commands: Vec<ScriptCommand>,
    command_index: usize,
    command_elapsed: f32,
    look: Vec2,
    started: bool,
    exit_after_script: bool,
    finished_elapsed: Option<f32>,
}

pub(crate) struct DiagnosticPlugin {
    script: Option<InputScript>,
    log: Option<File>,
    exit_after_script: bool,
    render_schedule: Option<RenderScheduleTelemetry>,
}

impl DiagnosticPlugin {
    pub(crate) fn new(
        script_path: Option<&str>,
        log_path: Option<&str>,
        exit_after_script: bool,
    ) -> Result<Self, String> {
        let script = script_path
            .map(|path| -> Result<InputScript, String> {
                let bytes = std::fs::read(path)
                    .map_err(|error| format!("failed to read input script {path}: {error}"))?;
                let script: InputScript = serde_json::from_slice(&bytes)
                    .map_err(|error| format!("failed to parse input script {path}: {error}"))?;
                validate_script(&script)?;
                Ok(script)
            })
            .transpose()?;
        let log = log_path
            .map(|path| {
                let path = Path::new(path);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|error| {
                        format!("failed to create animation log directory: {error}")
                    })?;
                }
                File::create(path).map_err(|error| {
                    format!("failed to create animation log {}: {error}", path.display())
                })
            })
            .transpose()?;
        if exit_after_script && script.is_none() {
            return Err("--exit-after-script requires --input-script".to_owned());
        }
        let render_schedule = log.as_ref().map(|_| RenderScheduleTelemetry::new());
        Ok(Self {
            script,
            log,
            exit_after_script,
            render_schedule,
        })
    }
}

impl Plugin for DiagnosticPlugin {
    fn build(&self, app: &mut App) {
        if let Some(file) = self.log.as_ref().and_then(|file| file.try_clone().ok()) {
            app.insert_resource(AnimationDiagnosticLog {
                writer: std::io::BufWriter::new(file),
                frame: 0,
            });
        }
        if let Some(telemetry) = &self.render_schedule {
            app.insert_resource(telemetry.clone());
        }
        if let Some(script) = &self.script {
            app.insert_resource(ScriptedInput {
                commands: script.commands.clone(),
                command_index: 0,
                command_elapsed: 0.0,
                look: Vec2::ZERO,
                started: false,
                exit_after_script: self.exit_after_script,
                finished_elapsed: None,
            })
            .init_resource::<DiagnosticInputStatus>()
            .add_systems(
                PreUpdate,
                (
                    suppress_physical_input.after(InputSystems),
                    drive_scripted_input.after(suppress_physical_input),
                ),
            );
        }
    }

    fn finish(&self, app: &mut App) {
        let Some(telemetry) = &self.render_schedule else {
            return;
        };
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.insert_resource(telemetry.clone()).add_systems(
            Render,
            record_render_schedule_completion.after(RenderSystems::Render),
        );
    }
}

fn suppress_physical_input(
    mut commands: Commands,
    players: Query<Entity, With<ClientPlayer>>,
    mut keyboard: ResMut<ButtonInput<KeyCode>>,
    mut mouse_buttons: ResMut<ButtonInput<MouseButton>>,
    mut mouse_motion: ResMut<AccumulatedMouseMotion>,
    mut mouse_scroll: ResMut<AccumulatedMouseScroll>,
) {
    keyboard.reset_all();
    mouse_buttons.reset_all();
    mouse_motion.delta = Vec2::ZERO;
    mouse_scroll.delta = Vec2::ZERO;
    for player in &players {
        commands
            .entity(player)
            .insert(ContextActivity::<Player>::INACTIVE);
    }
}

fn record_render_schedule_completion(telemetry: Res<RenderScheduleTelemetry>) {
    telemetry.record_completion();
}

fn validate_script(script: &InputScript) -> Result<(), String> {
    if script.commands.is_empty() {
        return Err("input script must contain at least one command".to_owned());
    }
    for command in &script.commands {
        match command {
            ScriptCommand::Rotate { degrees_right } if !degrees_right.is_finite() => {
                return Err("rotate degrees_right must be finite".to_owned());
            }
            ScriptCommand::Move {
                input_speed,
                duration_seconds,
                ..
            } if !(0.0..=1.0).contains(input_speed)
                || !duration_seconds.is_finite()
                || *duration_seconds <= 0.0 =>
            {
                return Err(
                    "move input_speed must be 0..=1 and duration_seconds must be positive"
                        .to_owned(),
                );
            }
            ScriptCommand::Wait { duration_seconds }
                if !duration_seconds.is_finite() || *duration_seconds <= 0.0 =>
            {
                return Err("wait duration_seconds must be positive".to_owned());
            }
            ScriptCommand::WaitForSignal { path } if path.trim().is_empty() => {
                return Err("wait_for_signal path must not be empty".to_owned());
            }
            _ => {}
        }
    }
    Ok(())
}

fn drive_scripted_input(
    time: Res<Time>,
    player: Query<(), With<ClientPlayer>>,
    mut script: ResMut<ScriptedInput>,
    mut input_override: ResMut<PlayerInputOverride>,
    mut status: ResMut<DiagnosticInputStatus>,
    mut exit: MessageWriter<AppExit>,
) {
    if player.is_empty() {
        input_override.0 = None;
        return;
    }
    if !script.started {
        script.started = true;
        info!(
            commands = script.commands.len(),
            "Started scripted real-client input"
        );
    }

    let delta = time.delta_secs().max(0.0);
    loop {
        let Some(command) = script.commands.get(script.command_index).cloned() else {
            input_override.0 = Some(PlayerInputRequest {
                look: script.look,
                ..default()
            });
            let exit_after_script = script.exit_after_script;
            let elapsed = script.finished_elapsed.get_or_insert(0.0);
            *elapsed += delta;
            if exit_after_script && *elapsed >= 0.25 {
                info!("Scripted real-client input completed");
                exit.write(AppExit::Success);
            }
            return;
        };

        if let ScriptCommand::Rotate { degrees_right } = &command {
            script.look.x = (script.look.x + degrees_right.to_radians() + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            script.command_index += 1;
            script.command_elapsed = 0.0;
            continue;
        }

        if let ScriptCommand::WaitForSignal { path } = &command {
            let request = PlayerInputRequest {
                look: script.look,
                ..default()
            };
            input_override.0 = Some(request);
            *status = DiagnosticInputStatus {
                command_index: script.command_index,
                command_kind: "wait_for_signal".to_owned(),
                command_elapsed_seconds: script.command_elapsed,
                request,
            };
            if Path::new(path).is_file() {
                script.command_index += 1;
                script.command_elapsed = 0.0;
                continue;
            }
            return;
        }

        script.command_elapsed += delta;
        let (kind, duration, movement) = match command {
            ScriptCommand::Move {
                direction,
                input_speed,
                duration_seconds,
            } => (
                "move",
                duration_seconds,
                Some(direction.vector() * input_speed),
            ),
            ScriptCommand::Wait { duration_seconds } => ("wait", duration_seconds, None),
            ScriptCommand::Rotate { .. } | ScriptCommand::WaitForSignal { .. } => unreachable!(),
        };
        let request = PlayerInputRequest {
            movement,
            look: script.look,
            jump: false,
            gait: MovementGait::Jog,
            weapon_guard: WeaponGuardState::Lowered,
        };
        input_override.0 = Some(request);
        let next_status = DiagnosticInputStatus {
            command_index: script.command_index,
            command_kind: kind.to_owned(),
            command_elapsed_seconds: script.command_elapsed,
            request,
        };
        *status = next_status;
        if script.command_elapsed >= duration {
            script.command_index += 1;
            script.command_elapsed = 0.0;
        }
        return;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_script_parses_and_validates() {
        let script: InputScript = serde_json::from_str(
            r#"{"commands":[{"type":"rotate","degrees_right":90.0},{"type":"move","direction":"forward","input_speed":0.5,"duration_seconds":2.0},{"type":"wait","duration_seconds":0.5}]}"#,
        )
        .unwrap();
        assert!(validate_script(&script).is_ok());
    }

    #[test]
    fn invalid_analogue_speed_is_rejected() {
        let script: InputScript = serde_json::from_str(
            r#"{"commands":[{"type":"move","input_speed":1.1,"duration_seconds":2.0}]}"#,
        )
        .unwrap();
        assert!(validate_script(&script).is_err());
    }

    #[test]
    fn signal_wait_requires_a_path() {
        let script: InputScript =
            serde_json::from_str(r#"{"commands":[{"type":"wait_for_signal","path":""}]}"#).unwrap();
        assert!(validate_script(&script).is_err());
    }

    #[test]
    fn scripted_mode_clears_physical_keyboard_and_mouse_input() {
        let mut world = World::new();
        let mut keyboard = ButtonInput::default();
        keyboard.press(KeyCode::KeyW);
        let mut mouse_buttons = ButtonInput::default();
        mouse_buttons.press(MouseButton::Left);
        world.insert_resource(keyboard);
        world.insert_resource(mouse_buttons);
        world.insert_resource(AccumulatedMouseMotion { delta: Vec2::ONE });
        world.insert_resource(AccumulatedMouseScroll {
            delta: Vec2::ONE,
            ..default()
        });

        world.run_system_cached(suppress_physical_input).unwrap();

        assert!(
            world
                .resource::<ButtonInput<KeyCode>>()
                .get_pressed()
                .next()
                .is_none()
        );
        assert!(
            world
                .resource::<ButtonInput<MouseButton>>()
                .get_pressed()
                .next()
                .is_none()
        );
        assert_eq!(world.resource::<AccumulatedMouseMotion>().delta, Vec2::ZERO);
        assert_eq!(world.resource::<AccumulatedMouseScroll>().delta, Vec2::ZERO);
    }
}
