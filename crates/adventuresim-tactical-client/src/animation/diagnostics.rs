use super::*;

#[cfg(not(target_family = "wasm"))]
use crate::camera::CameraAimState;
#[cfg(not(target_family = "wasm"))]
use adventuresim_tactical_netcode::client::{
    DirectControlState, LastPlayerInputRequest, WeaponGuardInputState,
};
#[cfg(not(target_family = "wasm"))]
use bevy::transform::helper::TransformHelper;

#[cfg(not(target_family = "wasm"))]
use std::io::{BufWriter, Write};
#[cfg(not(target_family = "wasm"))]
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

#[cfg(not(target_family = "wasm"))]
#[derive(Resource, Debug, Clone, Default, serde::Serialize)]
pub(crate) struct DiagnosticInputStatus {
    pub command_index: usize,
    pub command_kind: String,
    pub command_elapsed_seconds: f32,
    pub request: PlayerInputRequest,
}

#[cfg(not(target_family = "wasm"))]
#[derive(Resource)]
pub(crate) struct AnimationDiagnosticLog {
    path: PathBuf,
    writer: Option<BufWriter<std::fs::File>>,
    bytes_written: u64,
    max_bytes: u64,
    pub(crate) frame: u64,
}

#[cfg(not(target_family = "wasm"))]
const ANIMATION_LOG_MAX_BYTES: u64 = 32 * 1024 * 1024;

#[cfg(not(target_family = "wasm"))]
#[allow(dead_code)] // The gameplay client records render telemetry; the viewer only reads animation logs.
#[derive(Resource, Clone, Debug)]
pub(crate) struct RenderScheduleTelemetry(Arc<RenderScheduleShared>);

#[cfg(not(target_family = "wasm"))]
#[derive(Debug)]
struct RenderScheduleShared {
    started: Instant,
    count: AtomicU64,
    elapsed_micros: AtomicU64,
}

#[cfg(not(target_family = "wasm"))]
#[allow(dead_code)] // Construction and recording are native gameplay diagnostics entry points.
impl RenderScheduleTelemetry {
    pub(crate) fn new() -> Self {
        Self(Arc::new(RenderScheduleShared {
            started: Instant::now(),
            count: AtomicU64::new(0),
            elapsed_micros: AtomicU64::new(0),
        }))
    }

    pub(crate) fn record_completion(&self) {
        let elapsed = self.0.started.elapsed().as_micros();
        self.0
            .elapsed_micros
            .store(elapsed.min(u64::MAX as u128) as u64, Ordering::Release);
        self.0.count.fetch_add(1, Ordering::Release);
    }

    pub(crate) fn snapshot(&self) -> (u64, u64) {
        (
            self.0.count.load(Ordering::Acquire),
            self.0.elapsed_micros.load(Ordering::Acquire),
        )
    }
}

#[cfg(not(target_family = "wasm"))]
impl AnimationDiagnosticLog {
    pub(crate) fn new(path: PathBuf) -> std::io::Result<Self> {
        Self::with_max_bytes(path, ANIMATION_LOG_MAX_BYTES)
    }

    fn with_max_bytes(path: PathBuf, max_bytes: u64) -> std::io::Result<Self> {
        let writer = BufWriter::new(std::fs::File::create(&path)?);
        let mut log = Self {
            path,
            writer: Some(writer),
            bytes_written: 0,
            max_bytes: max_bytes.max(1024),
            frame: 0,
        };
        log.write_header(false)?;
        Ok(log)
    }

    fn previous_path(&self) -> PathBuf {
        self.path.with_extension("previous.jsonl")
    }

    fn write_header(&mut self, continued: bool) -> std::io::Result<()> {
        let header = serde_json::to_vec(&serde_json::json!({
            "record_type": "session_header",
            "schema": "adventuresim.animation.live",
            "schema_version": 2,
            "session_name": self.path.file_stem().and_then(|name| name.to_str()),
            "log_path": self.path,
            "continued": continued,
            "first_frame": self.frame,
            "max_segment_bytes": self.max_bytes,
            "segments_retained": 2,
            "started_unix_micros": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_micros().min(u64::MAX as u128) as u64)
                .unwrap_or_default(),
        }))?;
        let writer = self
            .writer
            .as_mut()
            .expect("animation log writer installed");
        writer.write_all(&header)?;
        writer.write_all(b"\n")?;
        self.bytes_written = header.len() as u64 + 1;
        writer.flush()
    }

    fn rotate(&mut self) -> std::io::Result<()> {
        if let Some(mut writer) = self.writer.take() {
            writer.flush()?;
        }
        let previous = self.previous_path();
        if previous.exists() {
            std::fs::remove_file(&previous)?;
        }
        if self.path.exists() {
            std::fs::rename(&self.path, previous)?;
        }
        self.writer = Some(BufWriter::new(std::fs::File::create(&self.path)?));
        self.write_header(true)
    }

    pub(crate) fn write(&mut self, mut value: serde_json::Value) {
        value["frame"] = self.frame.into();
        value["record_type"] = "frame".into();
        let mut encoded = serde_json::to_vec(&value)
            .expect("animation diagnostic snapshot should remain serializable");
        if encoded.len() as u64 + 1 >= self.max_bytes {
            encoded = serde_json::to_vec(&serde_json::json!({
                "record_type": "frame",
                "frame": self.frame,
                "truncated": true,
                "original_record_bytes": encoded.len(),
                "reason": "record_exceeded_segment_cap",
            }))
            .expect("animation truncation marker should remain serializable");
        }
        if self.bytes_written + encoded.len() as u64 + 1 > self.max_bytes {
            self.rotate()
                .expect("animation diagnostic log should remain rotatable");
        }
        let writer = self
            .writer
            .as_mut()
            .expect("animation log writer installed");
        writer
            .write_all(&encoded)
            .expect("animation diagnostic log should remain writable");
        writer
            .write_all(b"\n")
            .expect("animation diagnostic log should remain writable");
        self.bytes_written += encoded.len() as u64 + 1;
        if self.frame % 60 == 59 {
            writer
                .flush()
                .expect("animation diagnostic log should remain writable");
        }
        self.frame += 1;
    }
}

#[cfg(not(target_family = "wasm"))]
pub(super) fn log_animation_diagnostics(
    time: Res<Time>,
    clock: Res<ProceduralAnimationClock>,
    mut log: Option<ResMut<AnimationDiagnosticLog>>,
    input: Option<Res<DiagnosticInputStatus>>,
    guard_input: Option<Res<WeaponGuardInputState>>,
    direct_controls: Option<Res<DirectControlState>>,
    camera_aim: Option<Res<CameraAimState>>,
    last_player_input: Option<Res<LastPlayerInputRequest>>,
    render_schedule: Option<Res<RenderScheduleTelemetry>>,
    jitter_diagnostics: Option<Res<JointJitterDiagnostics>>,
    terrains: Query<&SceneTerrain>,
    transforms: TransformHelper,
    players: Query<
        (
            Entity,
            &Transform,
            &GlobalTransform,
            Option<&Rotation>,
            &SkeletonState,
            &PresentedSkeleton,
            &AnimationPlayback,
            Option<&semantic_route::SemanticRouteTrace>,
            Option<&HumanoidRig>,
            Option<&LegIkState>,
            Option<&RaisedFootworkState>,
        ),
        (With<Player>, With<crate::player::ClientPlayer>),
    >,
) {
    let Some(log) = log.as_mut() else {
        return;
    };
    let render_schedule_completion = render_schedule.as_deref().map(|telemetry| {
        let (count, elapsed_micros) = telemetry.snapshot();
        serde_json::json!({
            "count": count,
            "elapsed_micros": elapsed_micros,
        })
    });
    let wall_clock_unix_micros = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_micros().min(u64::MAX as u128) as u64)
        .unwrap_or_default();
    let terrain = terrains.iter().next();
    for (
        entity,
        transform,
        global_transform,
        physics_rotation,
        authoritative,
        presented,
        playback,
        semantic_route,
        rig,
        leg_ik,
        raised_footwork,
    ) in &players
    {
        let global_translation = global_transform.translation();
        let terrain_height = terrain.and_then(|terrain| terrain.height_at(global_translation.xz()));
        let evaluation = AnimationEvaluation::from_skeleton(presented);
        let leg_diagnostics = leg_ik.map(LegIkState::diagnostics);
        let left_raised_motion =
            raised_footwork.and_then(|state| state.foot_motion_diagnostic(true));
        let right_raised_motion =
            raised_footwork.and_then(|state| state.foot_motion_diagnostic(false));
        let left_leg_motion = leg_diagnostics.and_then(|state| state.left_presented_motion);
        let right_leg_motion = leg_diagnostics.and_then(|state| state.right_presented_motion);
        let semantic_tick = clock.semantic_step().0;
        let raised_selected =
            raised_footwork.is_some_and(|state| state.diagnostic_is_motion_owner(semantic_tick));
        let left_motion = if raised_selected {
            left_raised_motion
        } else {
            left_leg_motion
        };
        let right_motion = if raised_selected {
            right_raised_motion
        } else {
            right_leg_motion
        };
        let pelvis_motion = leg_diagnostics.map(|diagnostics| {
            let raised_owner = raised_selected && diagnostics.raised_pelvis_follower_valid;
            serde_json::json!({
                "selected_owner": if raised_owner { "raised" } else { "ordinary" },
                "position": if raised_owner { diagnostics.raised_pelvis_shift } else { diagnostics.pelvis_shift },
                "velocity": if raised_owner { diagnostics.raised_pelvis_shift_velocity } else { diagnostics.pelvis_shift_velocity },
                "acceleration": if raised_owner { diagnostics.raised_pelvis_shift_acceleration } else { diagnostics.pelvis_shift_acceleration },
                "raised_recovery_active": diagnostics.raised_pelvis_recovery_active,
                "raised_recovery_progress": diagnostics.raised_pelvis_recovery_progress,
            })
        });
        let lower_body =
            rig.map(|rig| lower_body_snapshot(rig, *global_transform, &transforms, terrain));
        let upper_body = rig.map(|rig| upper_body_snapshot(rig, *global_transform, &transforms));
        let bone_globals = rig.map(|rig| bone_global_snapshot(rig, &transforms));
        let clips = playback
            .clips
            .iter()
            .map(|clip| {
                serde_json::json!({
                    "weight": clip.weight,
                    "time_seconds": clip.time_seconds,
                    "mirrored_weight": clip.mirrored_weight,
                    "layer": format!("{:?}", clip.clip.layer).to_ascii_lowercase(),
                })
            })
            .collect::<Vec<_>>();
        log.write(serde_json::json!({
            "elapsed_seconds": time.elapsed_secs_f64(),
            "wall_clock_unix_micros": wall_clock_unix_micros,
            "render_delta_seconds": time.delta_secs(),
            "procedural_clock": clock.diagnostic_snapshot(),
            "render_schedule_completion": render_schedule_completion,
            "input": input.as_deref(),
            "last_emitted_player_input": last_player_input.as_deref().and_then(|input| input.0),
            "live_input": {
                "weapon_guard": guard_input.as_deref().map(|guard| format!("{:?}", guard.desired)),
                "direct_control": direct_controls.as_deref().map(|controls| serde_json::json!({
                    "pace": format!("{:?}", controls.pace),
                    "crouch": controls.crouch,
                    "jump_charge": controls.jump_charge,
                    "attack_just_pressed": controls.attack_just_pressed,
                    "alternate_attack": controls.alternate_attack,
                    "dodge_just_pressed": controls.dodge_just_pressed,
                    "quickstep_direction": controls.quickstep_direction,
                    "roll_just_pressed": controls.roll_just_pressed,
                    "downed_align": controls.downed_align,
                })),
                "camera_aim": camera_aim.as_deref().map(|aim| serde_json::json!({
                    "active": aim.active,
                    "camera_origin": aim.camera_origin,
                    "camera_target": aim.camera_target,
                    "camera_hit": aim.camera_hit.map(Entity::to_bits),
                    "muzzle_origin": aim.muzzle_origin,
                    "actual_target": aim.actual_target,
                    "actual_hit": aim.actual_hit.map(Entity::to_bits),
                    "blocked": aim.blocked,
                })),
            },
            "controller_transform": {
                "translation": transform.translation.to_array(),
                "rotation_xyzw": transform.rotation.to_array(),
            },
            "controller_global_transform": {
                "translation": global_translation.to_array(),
                "rotation_xyzw": global_transform.compute_transform().rotation.to_array(),
            },
            "controller_physics_rotation_xyzw": physics_rotation
                .map(|rotation| rotation.0.to_array()),
            "terrain_height": terrain_height,
            "controller_height_above_terrain": terrain_height
                .map(|height| global_translation.y - height),
            "authoritative": authoritative,
            "presented": &presented.state,
            "cadence_identity": presented.cadence_diagnostic_snapshot(),
            "presentation_phase_error_remaining": presented.phase_error_remaining,
            "presentation_phase_prediction_delta": presented.last_phase_prediction_delta,
            "presentation_phase_correction_delta": presented.last_phase_correction_delta,
            "presentation_phase_measurement_error": presented.last_phase_measurement_error,
            "presentation_phase_source_changed": presented.last_phase_source_changed,
            "evaluation": evaluation,
            "semantic_route": semantic_route.map(|trace| serde_json::json!({
                "requested_path": trace.requested_path,
                "path": trace.path,
                "inputs": &trace.inputs,
                "runtime_evaluated": trace.runtime_evaluated,
                "evaluation_equivalent": trace.evaluation == evaluation,
            })),
            "playback": {
                "use_authored_bind_pose": playback.use_authored_bind_pose,
                "whole_body_mirror": playback.whole_body_mirror,
                "ordinary_locomotion_active": playback.ordinary_locomotion_active,
                "clips": clips,
            },
            "leg_ik": leg_diagnostics,
            "pelvis_motion": pelvis_motion,
            "raised_ownership": raised_footwork.map(RaisedFootworkState::diagnostic_snapshot),
            "foot_motion": {
                "selected_source": if raised_selected { "raised_footwork" } else { "leg_ik" },
                "left": {
                    "selected": foot_motion_snapshot(left_motion, rig, &transforms, true),
                    "raised_candidate": left_raised_motion,
                    "leg_ik_candidate": left_leg_motion,
                },
                "right": {
                    "selected": foot_motion_snapshot(right_motion, rig, &transforms, false),
                    "raised_candidate": right_raised_motion,
                    "leg_ik_candidate": right_leg_motion,
                },
            },
            "lower_body": lower_body,
            "upper_body": upper_body,
            "bone_globals": bone_globals,
            "joint_jitter": jitter_diagnostics
                .as_deref()
                .map(|diagnostics| diagnostics.live_log_snapshot(entity)),
        }));
    }
}

#[cfg(not(target_family = "wasm"))]
fn bone_global_snapshot(rig: &HumanoidRig, transforms: &TransformHelper) -> serde_json::Value {
    let mut bones = serde_json::Map::with_capacity(BoneRole::ALL.len());
    for role in BoneRole::ALL {
        let Some(entity) = rig.get(&role).copied() else {
            continue;
        };
        let Ok(global) = transforms.compute_global_transform(entity) else {
            continue;
        };
        let transform = global.compute_transform();
        bones.insert(
            role.label().to_owned(),
            serde_json::json!({
                "entity": entity.to_bits(),
                "translation": transform.translation,
                "rotation_xyzw": transform.rotation.to_array(),
                "scale": transform.scale,
            }),
        );
    }
    serde_json::Value::Object(bones)
}

#[cfg(not(target_family = "wasm"))]
fn upper_body_snapshot(
    rig: &HumanoidRig,
    owner_global: GlobalTransform,
    transforms: &TransformHelper,
) -> serde_json::Value {
    let bone = |role: BoneRole| {
        let entity = *rig.get(&role)?;
        let global = transforms.compute_global_transform(entity).ok()?;
        let world = global.compute_transform();
        let owner_local = GlobalTransform::from(owner_global.affine().inverse() * global.affine())
            .compute_transform();
        Some(serde_json::json!({
            "entity": entity.to_bits(),
            "world": {
                "translation": world.translation,
                "rotation_xyzw": world.rotation.to_array(),
            },
            "owner_local": {
                "translation": owner_local.translation,
                "rotation_xyzw": owner_local.rotation.to_array(),
            },
        }))
    };
    serde_json::json!({
        "left": {
            "shoulder": bone(BoneRole::UpperArmLeft),
            "elbow": bone(BoneRole::ForearmLeft),
            "hand": bone(BoneRole::HandLeft),
        },
        "right": {
            "shoulder": bone(BoneRole::UpperArmRight),
            "elbow": bone(BoneRole::ForearmRight),
            "hand": bone(BoneRole::HandRight),
        },
    })
}

#[cfg(not(target_family = "wasm"))]
fn foot_motion_snapshot(
    motion: Option<FootMotionDiagnostic>,
    rig: Option<&HumanoidRig>,
    transforms: &TransformHelper,
    left: bool,
) -> Option<serde_json::Value> {
    let motion = motion?;
    let ankle_role = if left {
        BoneRole::FootLeft
    } else {
        BoneRole::FootRight
    };
    let rendered_ankle = rig
        .and_then(|rig| rig.get(&ankle_role))
        .and_then(|bone| transforms.compute_global_transform(*bone).ok())
        .map(|global| global.translation());
    let reach_distance = motion.solve_hip.map(|hip| hip.distance(motion.presented));
    let warning_reach = motion
        .upper_length
        .zip(motion.lower_length)
        .map(|(upper, lower)| {
            (upper * upper + lower * lower + 2.0 * upper * lower * 30.0_f32.to_radians().cos())
                .sqrt()
        });
    let hard_reach = motion
        .upper_length
        .zip(motion.lower_length)
        .map(|(upper, lower)| {
            (upper * upper + lower * lower + 2.0 * upper * lower * 20.0_f32.to_radians().cos())
                .sqrt()
        });
    let reach_disposition = match (reach_distance, warning_reach, hard_reach) {
        (Some(distance), _, Some(hard)) if distance >= hard => "hard",
        (Some(distance), Some(warning), _) if distance >= warning => "warning",
        (Some(_), _, _) => "within",
        _ => "unknown",
    };
    Some(serde_json::json!({
        "diagnostic": motion,
        "commanded_lag": motion.commanded.map(|target| target.distance(motion.presented)),
        "rendered_ankle": rendered_ankle,
        "target_render_error": rendered_ankle.map(|ankle| ankle.distance(motion.presented)),
        "reach_distance": reach_distance,
        "warning_reach": warning_reach,
        "hard_reach": hard_reach,
        "reach_disposition": reach_disposition,
    }))
}

#[cfg(not(target_family = "wasm"))]
fn lower_body_snapshot(
    rig: &HumanoidRig,
    owner_global: GlobalTransform,
    transforms: &TransformHelper,
    terrain: Option<&SceneTerrain>,
) -> serde_json::Value {
    let world_position = |role: BoneRole| {
        rig.get(&role)
            .and_then(|entity| transforms.compute_global_transform(*entity).ok())
            .map(|global| global.translation())
    };
    let bone = |role: BoneRole, sole_axis: Option<Vec3>| {
        let entity = *rig.get(&role)?;
        let global = transforms.compute_global_transform(entity).ok()?;
        let world = global.compute_transform();
        let owner_local_affine = owner_global.affine().inverse() * global.affine();
        let owner_local = GlobalTransform::from(owner_local_affine).compute_transform();
        let terrain_height = terrain.and_then(|terrain| terrain.height_at(world.translation.xz()));
        let terrain_normal = terrain.and_then(|terrain| terrain.normal_at(world.translation.xz()));
        let rendered_sole_normal = sole_axis.map(|axis| world.rotation * axis);
        Some(serde_json::json!({
            "entity": entity.to_bits(),
            "world": {
                "translation": world.translation,
                "rotation_xyzw": world.rotation.to_array(),
            },
            "owner_local": {
                "translation": owner_local.translation,
                "rotation_xyzw": owner_local.rotation.to_array(),
            },
            "terrain_height": terrain_height,
            "terrain_normal": terrain_normal,
            "rendered_sole_normal": rendered_sole_normal,
            "clearance": terrain_height.map(|height| world.translation.y - height),
        }))
    };
    let pelvis_world = world_position(BoneRole::Pelvis);
    let left_ankle_world = world_position(BoneRole::FootLeft);
    let right_ankle_world = world_position(BoneRole::FootRight);
    let owner_inverse = owner_global.affine().inverse();
    serde_json::json!({
        "root": bone(BoneRole::Root, None),
        "pelvis": bone(BoneRole::Pelvis, None),
        "left": {
            "hip": bone(BoneRole::ThighLeft, None),
            "knee": bone(BoneRole::ShinLeft, None),
            "ankle": bone(BoneRole::FootLeft, rig.sole_axis(true)),
            "toe": bone(BoneRole::ToeLeft, None),
            "ankle_from_visual_pelvis_world": left_ankle_world.zip(pelvis_world).map(|(ankle, pelvis)| ankle - pelvis),
            "ankle_owner_local": left_ankle_world.map(|ankle| owner_inverse.transform_point3(ankle)),
        },
        "right": {
            "hip": bone(BoneRole::ThighRight, None),
            "knee": bone(BoneRole::ShinRight, None),
            "ankle": bone(BoneRole::FootRight, rig.sole_axis(false)),
            "toe": bone(BoneRole::ToeRight, None),
            "ankle_from_visual_pelvis_world": right_ankle_world.zip(pelvis_world).map(|(ankle, pelvis)| ankle - pelvis),
            "ankle_owner_local": right_ankle_world.map(|ankle| owner_inverse.transform_point3(ankle)),
        },
    })
}

#[cfg(target_family = "wasm")]
pub(super) fn log_animation_diagnostics() {}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;

    fn unique_log_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "adventuresim-{name}-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn animation_log_starts_with_versioned_session_header() {
        let path = unique_log_path("animation-header");
        let mut log = AnimationDiagnosticLog::with_max_bytes(path.clone(), 4096).unwrap();
        log.write(serde_json::json!({"semantic_tick": 7}));
        log.writer.as_mut().unwrap().flush().unwrap();
        let records = std::fs::read_to_string(&path).unwrap();
        let mut records = records
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap());
        let header = records.next().unwrap();
        assert_eq!(header["schema"], "adventuresim.animation.live");
        assert_eq!(header["schema_version"], 2);
        assert_eq!(records.next().unwrap()["semantic_tick"], 7);
        drop(log);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn animation_log_rotation_retains_only_current_and_previous_segments() {
        let path = unique_log_path("animation-rotation");
        let previous = path.with_extension("previous.jsonl");
        let mut log = AnimationDiagnosticLog::with_max_bytes(path.clone(), 1024).unwrap();
        for tick in 0..80 {
            log.write(serde_json::json!({
                "semantic_tick": tick,
                "payload": "x".repeat(180),
            }));
        }
        log.writer.as_mut().unwrap().flush().unwrap();
        assert!(path.metadata().unwrap().len() <= 1024);
        assert!(previous.metadata().unwrap().len() <= 1024);
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("session_header")
        );
        assert!(
            std::fs::read_to_string(&previous)
                .unwrap()
                .contains("session_header")
        );
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("\"semantic_tick\":79")
        );
        drop(log);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(previous).unwrap();
    }

    #[test]
    fn oversized_animation_record_is_replaced_by_bounded_marker() {
        let path = unique_log_path("animation-oversized");
        let mut log = AnimationDiagnosticLog::with_max_bytes(path.clone(), 1024).unwrap();
        log.write(serde_json::json!({"payload": "x".repeat(4096)}));
        log.writer.as_mut().unwrap().flush().unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(path.metadata().unwrap().len() <= 1024);
        assert!(contents.contains("record_exceeded_segment_cap"));
        assert!(contents.contains("\"truncated\":true"));
        drop(log);
        std::fs::remove_file(path).unwrap();
    }
}
